//! VACUUM, ANALYZE y REINDEX desde la aplicación.
//!
//! Las tres son operaciones largas y ninguna puede correr dentro de una transacción abierta, así
//! que se ejecutan en una sesión propia ([`crate::conn::Session`]) sin `statement_timeout`, con su
//! `CancelToken` disponible y transmitiendo los `NOTICE` del servidor a medida que llegan. Sin eso
//! un `VACUUM` sobre una tabla grande sería una barra de progreso falsa durante veinte minutos.

use serde::{Deserialize, Serialize};

use crate::caps::ServerCaps;
use crate::conn::Session;
use crate::ddl::quote_ident;
use crate::error::{Error, Result};

/// Sobre qué se ejecuta la tarea.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    Database { name: String },
    Table { schema: String, name: String },
}

impl Target {
    fn sql(&self) -> String {
        match self {
            Target::Database { name } => quote_ident(name),
            Target::Table { schema, name } => {
                format!("{}.{}", quote_ident(schema), quote_ident(name))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Operation {
    #[serde(rename_all = "camelCase")]
    Vacuum {
        /// Reescribe la tabla entera y devuelve el espacio al sistema operativo. Toma un candado
        /// exclusivo: nadie puede leer ni escribir la tabla mientras dura.
        full: bool,
        freeze: bool,
        analyze: bool,
    },
    Analyze,
    #[serde(rename_all = "camelCase")]
    Reindex {
        /// Reconstruye sin bloquear las escrituras. Tarda más y puede dejar índices inválidos si
        /// falla, pero es la única opción viable sobre una base en uso.
        concurrently: bool,
    },
}

/// Arma la sentencia. Es una función pura para poder verificarla sin un servidor.
pub fn statement(operation: Operation, target: &Target, caps: &ServerCaps) -> Result<String> {
    let name = target.sql();

    match operation {
        Operation::Vacuum {
            full,
            freeze,
            analyze,
        } => {
            let mut options = vec!["VERBOSE"];
            if full {
                options.push("FULL");
            }
            if freeze {
                options.push("FREEZE");
            }
            if analyze {
                options.push("ANALYZE");
            }
            Ok(match target {
                // `VACUUM` sin objeto recorre toda la base a la que está conectada la sesión.
                Target::Database { .. } => format!("VACUUM ({})", options.join(", ")),
                Target::Table { .. } => format!("VACUUM ({}) {name}", options.join(", ")),
            })
        }

        Operation::Analyze => Ok(match target {
            Target::Database { .. } => "ANALYZE (VERBOSE)".to_owned(),
            Target::Table { .. } => format!("ANALYZE (VERBOSE) {name}"),
        }),

        Operation::Reindex { concurrently } => {
            if concurrently && !caps.has_reindex_concurrently() {
                return Err(Error::Config(format!(
                    "REINDEX CONCURRENTLY requiere PostgreSQL 12; este servidor es {}",
                    caps.version
                )));
            }

            let object = match target {
                Target::Database { .. } => "DATABASE",
                Target::Table { .. } => "TABLE",
            };
            let modifier = if concurrently { " CONCURRENTLY" } else { "" };
            Ok(format!("REINDEX (VERBOSE) {object}{modifier} {name}"))
        }
    }
}

/// Advertencia que corresponde mostrar antes de lanzar la operación, o `None` si no la hay.
///
/// Un `VACUUM FULL` sobre una tabla en producción la deja inaccesible mientras dura; enterarse
/// después no sirve de nada.
pub fn warning(operation: Operation) -> Option<&'static str> {
    match operation {
        Operation::Vacuum { full: true, .. } => Some(
            "VACUUM FULL reescribe la tabla completa y toma un candado exclusivo: nadie va a poder \
             leerla ni escribirla hasta que termine, y necesita espacio libre equivalente al \
             tamaño de la tabla.",
        ),
        Operation::Reindex {
            concurrently: false,
        } => Some(
            "REINDEX sin CONCURRENTLY bloquea las escrituras sobre la tabla mientras reconstruye \
             los índices.",
        ),
        _ => None,
    }
}

/// Ejecuta la sentencia en la sesión indicada.
///
/// Se usa `batch_execute` y no `query` porque `VACUUM` no admite el protocolo extendido: enviarlo
/// como sentencia preparada lo hace fallar.
pub async fn run(session: &Session, sql: &str) -> Result<()> {
    session.client().batch_execute(sql).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServerVersion;

    fn caps(num: i32) -> ServerCaps {
        ServerCaps {
            version: ServerVersion::from_num(num),
            current_user: "postgres".into(),
            current_database: "app".into(),
            is_superuser: true,
            can_signal_backends: true,
            can_read_all_stats: true,
        }
    }

    fn tabla() -> Target {
        Target::Table {
            schema: "public".into(),
            name: "clientes".into(),
        }
    }

    #[test]
    fn arma_el_vacuum_con_sus_opciones() {
        let sql = statement(
            Operation::Vacuum {
                full: false,
                freeze: false,
                analyze: true,
            },
            &tabla(),
            &caps(160_000),
        )
        .unwrap();
        assert_eq!(sql, "VACUUM (VERBOSE, ANALYZE) public.clientes");
    }

    #[test]
    fn el_vacuum_de_base_no_lleva_objeto() {
        let sql = statement(
            Operation::Vacuum {
                full: false,
                freeze: false,
                analyze: false,
            },
            &Target::Database { name: "app".into() },
            &caps(160_000),
        )
        .unwrap();
        assert_eq!(sql, "VACUUM (VERBOSE)");
    }

    #[test]
    fn cita_los_identificadores_que_lo_necesitan() {
        let sql = statement(
            Operation::Analyze,
            &Target::Table {
                schema: "public".into(),
                name: "Mi Tabla".into(),
            },
            &caps(160_000),
        )
        .unwrap();
        assert_eq!(sql, "ANALYZE (VERBOSE) public.\"Mi Tabla\"");
    }

    #[test]
    fn reindex_concurrently_lleva_el_modificador_despues_del_objeto() {
        let sql = statement(
            Operation::Reindex { concurrently: true },
            &tabla(),
            &caps(160_000),
        )
        .unwrap();
        assert_eq!(sql, "REINDEX (VERBOSE) TABLE CONCURRENTLY public.clientes");
    }

    #[test]
    fn avisa_antes_de_las_operaciones_que_bloquean() {
        assert!(warning(Operation::Vacuum {
            full: true,
            freeze: false,
            analyze: false
        })
        .is_some());
        assert!(warning(Operation::Reindex {
            concurrently: false
        })
        .is_some());
        assert!(warning(Operation::Analyze).is_none());
    }
}
