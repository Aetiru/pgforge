//! Crear, enganchar y separar particiones.
//!
//! Una partición es una tabla común con un límite declarado contra su madre, así que este módulo
//! no reimplementa nada de [`crate::ddl::table`]: solo arma la cláusula `PARTITION OF … FOR VALUES`
//! y los `ATTACH`/`DETACH` que la enganchan y la sueltan.
//!
//! Los valores del límite van **crudos**, misma frontera de confianza que el `DEFAULT` de una
//! columna: son expresiones (`'2024-01-01'`, `MINVALUE`, `MAXVALUE`, una llamada a función) y no
//! valores que se puedan parametrizar, y los valida el servidor al ejecutar.
//!
//! `DETACH PARTITION CONCURRENTLY` existe desde PostgreSQL 14 y lo decide [`ServerCaps`], no el
//! sitio de uso: sin él, separar una partición toma un `ACCESS EXCLUSIVE` sobre la tabla madre y
//! nadie puede leerla mientras dure. Como no corre adentro de una transacción, [`apply`] manda las
//! sentencias sueltas cuando hay alguna concurrente en la lista.

use crate::caps::ServerCaps;
use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::table::Statement;
use super::qualified;

use serde::{Deserialize, Serialize};

/// El límite de una partición.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PartitionBound {
    /// `FOR VALUES FROM (…) TO (…)`. Los extremos van crudos: admiten `MINVALUE` y `MAXVALUE`.
    Range { from: Vec<String>, to: Vec<String> },
    /// `FOR VALUES IN (…)`.
    List { values: Vec<String> },
    /// `FOR VALUES WITH (MODULUS m, REMAINDER r)`.
    Hash { modulus: i32, remainder: i32 },
    /// `DEFAULT`: se lleva todo lo que no entra en ninguna otra.
    Default,
}

/// Un cambio de partición pendiente.
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PartitionChange {
    /// Crea la tabla y la engancha en un solo paso.
    CreatePartition {
        parent_schema: String,
        parent: String,
        schema: String,
        name: String,
        bound: PartitionBound,
        /// Cuando la partición es a su vez particionada: `PARTITION BY RANGE (…)`, crudo.
        #[serde(default)]
        partition_by: Option<String>,
    },
    /// Engancha una tabla que ya existe. El servidor revisa que ninguna fila se salga del límite.
    AttachPartition {
        parent_schema: String,
        parent: String,
        schema: String,
        name: String,
        bound: PartitionBound,
    },
    DetachPartition {
        parent_schema: String,
        parent: String,
        schema: String,
        name: String,
        /// Sin bloquear a los lectores. Pide PostgreSQL 14 o más.
        #[serde(default)]
        concurrently: bool,
        /// Termina un `DETACH … CONCURRENTLY` que quedó a medias.
        #[serde(default)]
        finalize: bool,
    },
    DropPartition {
        schema: String,
        name: String,
        cascade: bool,
    },
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

fn bound_sql(bound: &PartitionBound) -> Result<String> {
    match bound {
        PartitionBound::Range { from, to } => {
            let from = require_values(from, "el extremo inicial")?;
            let to = require_values(to, "el extremo final")?;
            Ok(format!("FOR VALUES FROM ({from}) TO ({to})"))
        }
        PartitionBound::List { values } => {
            let values = require_values(values, "la lista de valores")?;
            Ok(format!("FOR VALUES IN ({values})"))
        }
        PartitionBound::Hash { modulus, remainder } => {
            if *modulus < 1 {
                return Err(Error::Config("el módulo es de al menos 1".to_owned()));
            }
            if *remainder < 0 || remainder >= modulus {
                return Err(Error::Config(
                    "el resto tiene que ser menor que el módulo".to_owned(),
                ));
            }
            Ok(format!(
                "FOR VALUES WITH (MODULUS {modulus}, REMAINDER {remainder})"
            ))
        }
        PartitionBound::Default => Ok("DEFAULT".to_owned()),
    }
}

fn require_values(values: &[String], what: &str) -> Result<String> {
    let values: Vec<&str> = values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect();

    if values.is_empty() {
        return Err(Error::Config(format!("falta {what} de la partición")));
    }
    Ok(values.join(", "))
}

/// Traduce los cambios pendientes a SQL.
///
/// Necesita las capacidades del servidor porque `DETACH … CONCURRENTLY` no existe antes de
/// PostgreSQL 14: pedirlo ahí devuelve un error claro en vez de un error de sintaxis del servidor.
pub fn statements(changes: &[PartitionChange], caps: &ServerCaps) -> Result<Vec<Statement>> {
    changes.iter().map(|change| one(change, caps)).collect()
}

fn one(change: &PartitionChange, caps: &ServerCaps) -> Result<Statement> {
    match change {
        PartitionChange::CreatePartition {
            parent_schema,
            parent,
            schema,
            name,
            bound,
            partition_by,
        } => {
            require_name(name)?;
            let sub = match partition_by.as_deref().map(str::trim) {
                Some(by) if !by.is_empty() => format!(" PARTITION BY {by}"),
                _ => String::new(),
            };
            Ok(statement(format!(
                "CREATE TABLE {}\n    PARTITION OF {}\n    {}{sub}",
                qualified(schema, name),
                qualified(parent_schema, parent),
                bound_sql(bound)?
            )))
        }
        PartitionChange::AttachPartition {
            parent_schema,
            parent,
            schema,
            name,
            bound,
        } => Ok(statement(format!(
            "ALTER TABLE {}\n    ATTACH PARTITION {} {}",
            qualified(parent_schema, parent),
            qualified(schema, name),
            bound_sql(bound)?
        ))),
        PartitionChange::DetachPartition {
            parent_schema,
            parent,
            schema,
            name,
            concurrently,
            finalize,
        } => {
            if *concurrently && *finalize {
                return Err(Error::Config(
                    "`CONCURRENTLY` y `FINALIZE` no van juntos: el segundo termina lo que dejó \
                     a medias el primero"
                        .to_owned(),
                ));
            }
            if *concurrently && !caps.has_detach_partition_concurrently() {
                return Err(Error::Config(format!(
                    "separar una partición sin bloquear pide PostgreSQL 14 o más; este servidor \
                     es {}",
                    caps.version.major()
                )));
            }
            let modifier = if *concurrently {
                " CONCURRENTLY"
            } else if *finalize {
                " FINALIZE"
            } else {
                ""
            };
            Ok(statement(format!(
                "ALTER TABLE {}\n    DETACH PARTITION {}{modifier}",
                qualified(parent_schema, parent),
                qualified(schema, name)
            )))
        }
        PartitionChange::DropPartition {
            schema,
            name,
            cascade,
        } => Ok(statement(format!(
            "DROP TABLE {}{}",
            qualified(schema, name),
            if *cascade { " CASCADE" } else { "" }
        ))),
    }
}

fn require_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Config("falta el nombre".to_owned()));
    }
    Ok(())
}

/// `true` si la lista lleva algún `DETACH … CONCURRENTLY`, que no admite transacción.
fn needs_autocommit(changes: &[PartitionChange]) -> bool {
    changes.iter().any(|change| {
        matches!(
            change,
            PartitionChange::DetachPartition {
                concurrently: true,
                ..
            }
        )
    })
}

/// Aplica los cambios.
///
/// En una transacción, como el resto del DDL, salvo que haya un `DETACH … CONCURRENTLY`: ese no
/// corre adentro de un bloque transaccional y obliga a mandar todo suelto. Mismo criterio que
/// [`crate::ddl::index`] con `CREATE INDEX CONCURRENTLY`.
pub async fn apply(
    handle: &ServerHandle,
    database: &str,
    changes: &[PartitionChange],
) -> Result<()> {
    let statements = statements(changes, &handle.caps)?;
    let mut client = handle.client(database).await?;

    if needs_autocommit(changes) {
        for statement in &statements {
            client.batch_execute(&statement.sql).await?;
        }
        return Ok(());
    }

    let transaction = client.transaction().await?;
    for statement in &statements {
        transaction.batch_execute(&statement.sql).await?;
    }
    transaction.commit().await?;
    Ok(())
}

/// Una partición de una tabla, tal como la ve el panel de detalle.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionInfo {
    pub schema: String,
    pub name: String,
    /// El límite tal como lo escribe el servidor: `FOR VALUES FROM ('2024-01-01') TO (…)`.
    pub bound: String,
    /// `true` si la partición está a su vez particionada.
    pub partitioned: bool,
}

/// Cómo está particionada una tabla y qué particiones tiene.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitioningInfo {
    /// La estrategia tal como la escribe el servidor: `RANGE (creado)`, `LIST (region)`, …
    pub strategy: String,
    pub partitions: Vec<PartitionInfo>,
}

/// Lee la partición de una tabla madre.
pub async fn info(handle: &ServerHandle, database: &str, oid: u32) -> Result<PartitioningInfo> {
    let client = handle.client(database).await?;

    let row = client
        .query_opt(
            "SELECT pg_catalog.pg_get_partkeydef($1)",
            &[&oid],
        )
        .await?;

    let strategy: Option<String> = row.and_then(|row| row.get(0));
    let strategy = strategy
        .ok_or_else(|| Error::Config("la tabla no está particionada".to_owned()))?;

    let partitions = client
        .query(
            "SELECT n.nspname::text,
                    c.relname::text,
                    pg_catalog.pg_get_expr(c.relpartbound, c.oid),
                    c.relkind = 'p'
               FROM pg_catalog.pg_inherits i
               JOIN pg_catalog.pg_class c ON c.oid = i.inhrelid
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
              WHERE i.inhparent = $1
              ORDER BY n.nspname, c.relname",
            &[&oid],
        )
        .await?
        .into_iter()
        .map(|row| PartitionInfo {
            schema: row.get(0),
            name: row.get(1),
            bound: row.get::<_, Option<String>>(2).unwrap_or_default(),
            partitioned: row.get(3),
        })
        .collect();

    Ok(PartitioningInfo {
        strategy,
        partitions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::ServerVersion;

    fn caps(major: i32) -> ServerCaps {
        ServerCaps {
            version: ServerVersion::from_num(major * 10_000),
            current_user: "postgres".to_owned(),
            current_database: "postgres".to_owned(),
            is_superuser: true,
            can_signal_backends: true,
            can_read_all_stats: true,
        }
    }

    fn one_statement(change: PartitionChange) -> Statement {
        statements(&[change], &caps(16))
            .expect("tenía que generar la sentencia")
            .remove(0)
    }

    #[test]
    fn crea_una_particion_por_rango() {
        let statement = one_statement(PartitionChange::CreatePartition {
            parent_schema: "public".into(),
            parent: "ventas".into(),
            schema: "public".into(),
            name: "ventas_2024".into(),
            bound: PartitionBound::Range {
                from: vec!["'2024-01-01'".into()],
                to: vec!["'2025-01-01'".into()],
            },
            partition_by: None,
        });
        assert_eq!(
            statement.sql,
            "CREATE TABLE public.ventas_2024\n    PARTITION OF public.ventas\n    \
             FOR VALUES FROM ('2024-01-01') TO ('2025-01-01')"
        );
    }

    #[test]
    fn crea_una_particion_que_a_su_vez_se_particiona() {
        let statement = one_statement(PartitionChange::CreatePartition {
            parent_schema: "public".into(),
            parent: "ventas".into(),
            schema: "public".into(),
            name: "ventas_2024".into(),
            bound: PartitionBound::Range {
                from: vec!["'2024-01-01'".into()],
                to: vec!["'2025-01-01'".into()],
            },
            partition_by: Some("LIST (region)".into()),
        });
        assert!(
            statement.sql.ends_with(" PARTITION BY LIST (region)"),
            "{}",
            statement.sql
        );
    }

    #[test]
    fn crea_particiones_por_lista_hash_y_por_omision() {
        let statement = one_statement(PartitionChange::CreatePartition {
            parent_schema: "public".into(),
            parent: "ventas".into(),
            schema: "public".into(),
            name: "ventas_sur".into(),
            bound: PartitionBound::List {
                values: vec!["'sur'".into(), "'patagonia'".into()],
            },
            partition_by: None,
        });
        assert!(
            statement.sql.ends_with("FOR VALUES IN ('sur', 'patagonia')"),
            "{}",
            statement.sql
        );

        let statement = one_statement(PartitionChange::CreatePartition {
            parent_schema: "public".into(),
            parent: "ventas".into(),
            schema: "public".into(),
            name: "ventas_0".into(),
            bound: PartitionBound::Hash {
                modulus: 4,
                remainder: 0,
            },
            partition_by: None,
        });
        assert!(
            statement
                .sql
                .ends_with("FOR VALUES WITH (MODULUS 4, REMAINDER 0)"),
            "{}",
            statement.sql
        );

        let statement = one_statement(PartitionChange::CreatePartition {
            parent_schema: "public".into(),
            parent: "ventas".into(),
            schema: "public".into(),
            name: "ventas_resto".into(),
            bound: PartitionBound::Default,
            partition_by: None,
        });
        assert!(statement.sql.ends_with("DEFAULT"), "{}", statement.sql);
    }

    #[test]
    fn un_rango_sin_extremos_no_se_genera() {
        assert!(statements(
            &[PartitionChange::CreatePartition {
                parent_schema: "public".into(),
                parent: "ventas".into(),
                schema: "public".into(),
                name: "ventas_2024".into(),
                bound: PartitionBound::Range {
                    from: vec!["  ".into()],
                    to: vec!["'2025-01-01'".into()],
                },
                partition_by: None,
            }],
            &caps(16)
        )
        .is_err());
    }

    #[test]
    fn un_hash_con_resto_mayor_que_el_modulo_no_se_genera() {
        assert!(statements(
            &[PartitionChange::CreatePartition {
                parent_schema: "public".into(),
                parent: "ventas".into(),
                schema: "public".into(),
                name: "ventas_0".into(),
                bound: PartitionBound::Hash {
                    modulus: 4,
                    remainder: 4,
                },
                partition_by: None,
            }],
            &caps(16)
        )
        .is_err());
    }

    #[test]
    fn engancha_una_tabla_existente() {
        let statement = one_statement(PartitionChange::AttachPartition {
            parent_schema: "public".into(),
            parent: "ventas".into(),
            schema: "archivo".into(),
            name: "ventas_2023".into(),
            bound: PartitionBound::Range {
                from: vec!["'2023-01-01'".into()],
                to: vec!["'2024-01-01'".into()],
            },
        });
        assert_eq!(
            statement.sql,
            "ALTER TABLE public.ventas\n    ATTACH PARTITION archivo.ventas_2023 \
             FOR VALUES FROM ('2023-01-01') TO ('2024-01-01')"
        );
    }

    #[test]
    fn separa_una_particion() {
        let statement = one_statement(PartitionChange::DetachPartition {
            parent_schema: "public".into(),
            parent: "ventas".into(),
            schema: "public".into(),
            name: "ventas_2023".into(),
            concurrently: false,
            finalize: false,
        });
        assert_eq!(
            statement.sql,
            "ALTER TABLE public.ventas\n    DETACH PARTITION public.ventas_2023"
        );
    }

    #[test]
    fn separa_sin_bloquear_solo_desde_la_14() {
        let sin_bloquear = PartitionChange::DetachPartition {
            parent_schema: "public".into(),
            parent: "ventas".into(),
            schema: "public".into(),
            name: "ventas_2023".into(),
            concurrently: true,
            finalize: false,
        };

        let statement = statements(std::slice::from_ref(&sin_bloquear), &caps(14))
            .expect("en la 14 tenía que generarse")
            .remove(0);
        assert!(
            statement.sql.ends_with(" CONCURRENTLY"),
            "{}",
            statement.sql
        );

        assert!(statements(&[sin_bloquear], &caps(13)).is_err());
    }

    #[test]
    fn concurrently_y_finalize_no_van_juntos() {
        assert!(statements(
            &[PartitionChange::DetachPartition {
                parent_schema: "public".into(),
                parent: "ventas".into(),
                schema: "public".into(),
                name: "ventas_2023".into(),
                concurrently: true,
                finalize: true,
            }],
            &caps(16)
        )
        .is_err());
    }

    #[test]
    fn termina_un_detach_a_medias() {
        let statement = one_statement(PartitionChange::DetachPartition {
            parent_schema: "public".into(),
            parent: "ventas".into(),
            schema: "public".into(),
            name: "ventas_2023".into(),
            concurrently: false,
            finalize: true,
        });
        assert!(statement.sql.ends_with(" FINALIZE"), "{}", statement.sql);
    }

    #[test]
    fn borra_una_particion() {
        let statement = one_statement(PartitionChange::DropPartition {
            schema: "public".into(),
            name: "ventas_2023".into(),
            cascade: false,
        });
        assert_eq!(statement.sql, "DROP TABLE public.ventas_2023");
    }

    #[test]
    fn el_detach_concurrente_obliga_a_mandar_todo_suelto() {
        let concurrente = [PartitionChange::DetachPartition {
            parent_schema: "public".into(),
            parent: "ventas".into(),
            schema: "public".into(),
            name: "ventas_2023".into(),
            concurrently: true,
            finalize: false,
        }];
        assert!(needs_autocommit(&concurrente));

        let normal = [PartitionChange::DropPartition {
            schema: "public".into(),
            name: "ventas_2023".into(),
            cascade: false,
        }];
        assert!(!needs_autocommit(&normal));
    }

    #[test]
    fn cita_los_identificadores_que_lo_necesitan() {
        let statement = one_statement(PartitionChange::AttachPartition {
            parent_schema: "mi esquema".into(),
            parent: "Ventas".into(),
            schema: "mi esquema".into(),
            name: "Ventas 2023".into(),
            bound: PartitionBound::Default,
        });
        assert_eq!(
            statement.sql,
            "ALTER TABLE \"mi esquema\".\"Ventas\"\n    \
             ATTACH PARTITION \"mi esquema\".\"Ventas 2023\" DEFAULT"
        );
    }
}
