//! Crear, cambiar, reiniciar y borrar secuencias.
//!
//! Una secuencia no tiene partes que se agreguen de a una, como las columnas de una tabla: son
//! siete parámetros que se escriben igual al crearla que al cambiarla. Por eso `CREATE` y `ALTER`
//! comparten [`SequenceOptions`] y no hay un cambio por parámetro.
//!
//! Cada opción es un `Option`: `None` significa «no la toques» y no «ponela en el valor por
//! omisión». En un `ALTER` la diferencia es todo —mandar el valor por omisión pisaría lo que el
//! usuario configuró—, y en un `CREATE` deja que el servidor elija, que es lo que hay que hacer
//! cuando el diálogo no preguntó por ese campo.
//!
//! `RESTART` va aparte de `start`: `START WITH` cambia a dónde vuelve la secuencia el día que la
//! reinicien, mientras que `RESTART` la mueve ahora. Confundirlos es el error clásico con
//! secuencias, así que son dos campos distintos y no uno con bandera.
//!
//! El tipo de dato va **crudo**, misma frontera de confianza que el tipo de una columna: no se
//! puede parametrizar en DDL, lo valida el servidor al ejecutar, y lo ejecuta el propio usuario con
//! sus propios privilegios.

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::table::Statement;
use super::{qualified, quote_ident, role_name};

use serde::{Deserialize, Serialize};

/// La columna que posee la secuencia: al borrarse la columna se borra la secuencia con ella.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedBy {
    pub schema: String,
    pub table: String,
    pub column: String,
}

/// A qué columna se ata la secuencia, o `None` para desatarla.
///
/// Es un enum y no un `Option<Option<OwnedBy>>` porque serde colapsa el `null` de JSON en el
/// `Option` de afuera: «desatala» y «no la toques» llegarían indistinguibles desde la interfaz.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SequenceOwner {
    /// `OWNED BY NONE`.
    None,
    Column {
        schema: String,
        table: String,
        column: String,
    },
}

/// Los parámetros de una secuencia. `None` en cualquiera es «no lo toques».
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceOptions {
    /// `smallint`, `integer` o `bigint`. Va crudo: lo valida el servidor.
    #[serde(default)]
    pub data_type: Option<String>,
    #[serde(default)]
    pub increment: Option<i64>,
    #[serde(default)]
    pub min_value: Option<i64>,
    #[serde(default)]
    pub max_value: Option<i64>,
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub cache: Option<i64>,
    #[serde(default)]
    pub cycle: Option<bool>,
    /// A qué columna pertenece. `None` no la toca.
    #[serde(default)]
    pub owned_by: Option<SequenceOwner>,
}

/// Un cambio de secuencia pendiente.
#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SequenceChange {
    CreateSequence {
        schema: String,
        name: String,
        /// `CREATE SEQUENCE IF NOT EXISTS`.
        #[serde(default)]
        if_not_exists: bool,
        options: SequenceOptions,
    },
    AlterSequence {
        schema: String,
        name: String,
        options: SequenceOptions,
    },
    /// Mueve la secuencia ahora. `None` la manda al `START WITH` que tenga configurado.
    RestartSequence {
        schema: String,
        name: String,
        #[serde(default)]
        value: Option<i64>,
    },
    RenameSequence {
        schema: String,
        name: String,
        new_name: String,
    },
    SetSequenceSchema {
        schema: String,
        name: String,
        new_schema: String,
    },
    SetSequenceOwner {
        schema: String,
        name: String,
        owner: String,
    },
    DropSequence {
        schema: String,
        name: String,
        cascade: bool,
    },
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

/// Traduce los cambios pendientes a SQL.
pub fn statements(changes: &[SequenceChange]) -> Result<Vec<Statement>> {
    changes.iter().map(one).collect()
}

fn one(change: &SequenceChange) -> Result<Statement> {
    match change {
        SequenceChange::CreateSequence {
            schema,
            name,
            if_not_exists,
            options,
        } => {
            require_name(name)?;
            let clauses = option_clauses(options)?;
            Ok(statement(format!(
                "CREATE SEQUENCE {}{}{}",
                if *if_not_exists { "IF NOT EXISTS " } else { "" },
                qualified(schema, name),
                clauses
            )))
        }
        SequenceChange::AlterSequence {
            schema,
            name,
            options,
        } => {
            let clauses = option_clauses(options)?;
            if clauses.is_empty() {
                return Err(Error::Config(
                    "no hay nada que cambiar en la secuencia".to_owned(),
                ));
            }
            Ok(statement(format!(
                "ALTER SEQUENCE {}{}",
                qualified(schema, name),
                clauses
            )))
        }
        SequenceChange::RestartSequence {
            schema,
            name,
            value,
        } => Ok(statement(format!(
            "ALTER SEQUENCE {} RESTART{}",
            qualified(schema, name),
            match value {
                Some(value) => format!(" WITH {value}"),
                None => String::new(),
            }
        ))),
        SequenceChange::RenameSequence {
            schema,
            name,
            new_name,
        } => {
            require_name(new_name)?;
            Ok(statement(format!(
                "ALTER SEQUENCE {} RENAME TO {}",
                qualified(schema, name),
                quote_ident(new_name)
            )))
        }
        SequenceChange::SetSequenceSchema {
            schema,
            name,
            new_schema,
        } => {
            require_name(new_schema)?;
            Ok(statement(format!(
                "ALTER SEQUENCE {} SET SCHEMA {}",
                qualified(schema, name),
                quote_ident(new_schema)
            )))
        }
        SequenceChange::SetSequenceOwner {
            schema,
            name,
            owner,
        } => {
            require_name(owner)?;
            Ok(statement(format!(
                "ALTER SEQUENCE {} OWNER TO {}",
                qualified(schema, name),
                role_name(owner)
            )))
        }
        SequenceChange::DropSequence {
            schema,
            name,
            cascade,
        } => Ok(statement(format!(
            "DROP SEQUENCE {}{}",
            qualified(schema, name),
            if *cascade { " CASCADE" } else { "" }
        ))),
    }
}

/// Las cláusulas comunes a `CREATE` y `ALTER`, en el orden en que las escribe el servidor.
///
/// Son las mismas de los dos lados —`AS bigint`, `INCREMENT BY`, `MINVALUE`…— y por eso las arma
/// una sola función: la diferencia entre crear y cambiar no está en la sintaxis sino en qué campos
/// vienen en `None`.
fn option_clauses(options: &SequenceOptions) -> Result<String> {
    let mut parts = Vec::new();

    if let Some(data_type) = options.data_type.as_ref() {
        let data_type = data_type.trim();
        if data_type.is_empty() {
            return Err(Error::Config(
                "el tipo de la secuencia no puede estar vacío".to_owned(),
            ));
        }
        parts.push(format!("AS {data_type}"));
    }

    if let Some(increment) = options.increment {
        if increment == 0 {
            return Err(Error::Config(
                "el incremento de una secuencia no puede ser cero".to_owned(),
            ));
        }
        parts.push(format!("INCREMENT BY {increment}"));
    }

    if let (Some(min), Some(max)) = (options.min_value, options.max_value) {
        if min > max {
            return Err(Error::Config(
                "el mínimo de la secuencia es mayor que el máximo".to_owned(),
            ));
        }
    }

    if let Some(min) = options.min_value {
        parts.push(format!("MINVALUE {min}"));
    }

    if let Some(max) = options.max_value {
        parts.push(format!("MAXVALUE {max}"));
    }

    if let Some(start) = options.start {
        parts.push(format!("START WITH {start}"));
    }

    if let Some(cache) = options.cache {
        if cache < 1 {
            return Err(Error::Config(
                "el caché de una secuencia es de al menos 1".to_owned(),
            ));
        }
        parts.push(format!("CACHE {cache}"));
    }

    if let Some(cycle) = options.cycle {
        parts.push(if cycle { "CYCLE" } else { "NO CYCLE" }.to_owned());
    }

    if let Some(owned_by) = options.owned_by.as_ref() {
        parts.push(match owned_by {
            SequenceOwner::Column {
                schema,
                table,
                column,
            } => format!(
                "OWNED BY {}.{}",
                qualified(schema, table),
                quote_ident(column)
            ),
            SequenceOwner::None => "OWNED BY NONE".to_owned(),
        });
    }

    if parts.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!(" {}", parts.join(" ")))
    }
}

fn require_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Config("falta el nombre".to_owned()));
    }
    Ok(())
}

/// Aplica los cambios en una sola transacción: mismo molde que `table::apply`.
pub async fn apply(
    handle: &ServerHandle,
    database: &str,
    changes: &[SequenceChange],
) -> Result<()> {
    let statements = statements(changes)?;
    let mut client = handle.client(database).await?;
    let transaction = client.transaction().await?;

    for statement in &statements {
        transaction.batch_execute(&statement.sql).await?;
    }

    transaction.commit().await?;
    Ok(())
}

/// Lo que hay que mostrar de una secuencia al abrir «Editar».
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceInfo {
    pub schema: String,
    pub name: String,
    pub owner: String,
    pub data_type: String,
    pub start: i64,
    pub increment: i64,
    pub min_value: i64,
    pub max_value: i64,
    pub cache: i64,
    pub cycle: bool,
    /// El valor actual. Es `None` cuando la secuencia todavía no se usó, y también cuando el rol
    /// conectado no tiene privilegio para leerla: `pg_sequences` devuelve nulo en los dos casos y
    /// no distingue cuál fue.
    pub last_value: Option<i64>,
    pub owned_by: Option<OwnedBy>,
    pub comment: Option<String>,
}

/// Lee la definición de una secuencia.
///
/// Sale de `pg_sequences` y no de un `SELECT` contra la secuencia misma: esa vista ya reúne los
/// parámetros con el valor actual, y leerla no consume un número —`nextval` sí lo haría—.
pub async fn info(handle: &ServerHandle, database: &str, oid: u32) -> Result<SequenceInfo> {
    let client = handle.client(database).await?;

    let row = client
        .query_one(
            "SELECT s.schemaname::text,
                    s.sequencename::text,
                    s.sequenceowner::text,
                    pg_catalog.format_type(q.seqtypid, NULL),
                    s.start_value,
                    s.increment_by,
                    s.min_value,
                    s.max_value,
                    s.cache_size,
                    s.cycle,
                    s.last_value,
                    pg_catalog.obj_description(c.oid, 'pg_class')
               FROM pg_catalog.pg_class c
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
               JOIN pg_catalog.pg_sequences s
                 ON s.schemaname = n.nspname AND s.sequencename = c.relname
               JOIN pg_catalog.pg_sequence q ON q.seqrelid = c.oid
              WHERE c.oid = $1",
            &[&oid],
        )
        .await?;

    // La columna dueña vive en `pg_depend` con `deptype = 'a'` (auto): es la dependencia que hace
    // que borrar la columna se lleve la secuencia puesta.
    let owned = client
        .query_opt(
            "SELECT n.nspname::text, c.relname::text, a.attname::text
               FROM pg_catalog.pg_depend d
               JOIN pg_catalog.pg_class c ON c.oid = d.refobjid
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
               JOIN pg_catalog.pg_attribute a
                 ON a.attrelid = d.refobjid AND a.attnum = d.refobjsubid
              WHERE d.classid = 'pg_class'::regclass
                AND d.objid = $1
                AND d.refclassid = 'pg_class'::regclass
                AND d.deptype IN ('a', 'i')",
            &[&oid],
        )
        .await?;

    Ok(SequenceInfo {
        schema: row.get(0),
        name: row.get(1),
        owner: row.get(2),
        data_type: row.get(3),
        start: row.get(4),
        increment: row.get(5),
        min_value: row.get(6),
        max_value: row.get(7),
        cache: row.get(8),
        cycle: row.get(9),
        last_value: row.get(10),
        owned_by: owned.map(|row| OwnedBy {
            schema: row.get(0),
            table: row.get(1),
            column: row.get(2),
        }),
        comment: row.get(11),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_statement(change: SequenceChange) -> Statement {
        statements(&[change])
            .expect("tenía que generar la sentencia")
            .remove(0)
    }

    fn options() -> SequenceOptions {
        SequenceOptions::default()
    }

    #[test]
    fn crea_una_secuencia_sin_opciones() {
        let statement = one_statement(SequenceChange::CreateSequence {
            schema: "public".into(),
            name: "clientes_id_seq".into(),
            if_not_exists: false,
            options: options(),
        });
        assert_eq!(statement.sql, "CREATE SEQUENCE public.clientes_id_seq");
    }

    #[test]
    fn crea_una_secuencia_con_todas_las_opciones() {
        let statement = one_statement(SequenceChange::CreateSequence {
            schema: "public".into(),
            name: "folios".into(),
            if_not_exists: true,
            options: SequenceOptions {
                data_type: Some("integer".into()),
                increment: Some(10),
                min_value: Some(100),
                max_value: Some(1000),
                start: Some(100),
                cache: Some(5),
                cycle: Some(true),
                owned_by: Some(SequenceOwner::Column {
                    schema: "public".into(),
                    table: "comprobantes".into(),
                    column: "folio".into(),
                }),
            },
        });
        assert_eq!(
            statement.sql,
            "CREATE SEQUENCE IF NOT EXISTS public.folios AS integer INCREMENT BY 10 \
             MINVALUE 100 MAXVALUE 1000 START WITH 100 CACHE 5 CYCLE \
             OWNED BY public.comprobantes.folio"
        );
    }

    #[test]
    fn desata_la_secuencia_de_su_columna() {
        let statement = one_statement(SequenceChange::AlterSequence {
            schema: "public".into(),
            name: "folios".into(),
            options: SequenceOptions {
                owned_by: Some(SequenceOwner::None),
                ..options()
            },
        });
        assert_eq!(statement.sql, "ALTER SEQUENCE public.folios OWNED BY NONE");
    }

    #[test]
    fn un_alter_sin_opciones_no_se_genera() {
        assert!(statements(&[SequenceChange::AlterSequence {
            schema: "public".into(),
            name: "folios".into(),
            options: options(),
        }])
        .is_err());
    }

    #[test]
    fn el_incremento_cero_no_se_genera() {
        assert!(statements(&[SequenceChange::AlterSequence {
            schema: "public".into(),
            name: "folios".into(),
            options: SequenceOptions {
                increment: Some(0),
                ..options()
            },
        }])
        .is_err());
    }

    #[test]
    fn el_minimo_mayor_que_el_maximo_no_se_genera() {
        assert!(statements(&[SequenceChange::AlterSequence {
            schema: "public".into(),
            name: "folios".into(),
            options: SequenceOptions {
                min_value: Some(10),
                max_value: Some(1),
                ..options()
            },
        }])
        .is_err());
    }

    #[test]
    fn el_cache_menor_que_uno_no_se_genera() {
        assert!(statements(&[SequenceChange::AlterSequence {
            schema: "public".into(),
            name: "folios".into(),
            options: SequenceOptions {
                cache: Some(0),
                ..options()
            },
        }])
        .is_err());
    }

    #[test]
    fn reinicia_con_y_sin_valor() {
        let statement = one_statement(SequenceChange::RestartSequence {
            schema: "public".into(),
            name: "folios".into(),
            value: None,
        });
        assert_eq!(statement.sql, "ALTER SEQUENCE public.folios RESTART");

        let statement = one_statement(SequenceChange::RestartSequence {
            schema: "public".into(),
            name: "folios".into(),
            value: Some(1),
        });
        assert_eq!(statement.sql, "ALTER SEQUENCE public.folios RESTART WITH 1");
    }

    #[test]
    fn renombra_mueve_y_cambia_de_dueno() {
        let statement = one_statement(SequenceChange::RenameSequence {
            schema: "public".into(),
            name: "folios".into(),
            new_name: "folios_viejos".into(),
        });
        assert_eq!(
            statement.sql,
            "ALTER SEQUENCE public.folios RENAME TO folios_viejos"
        );

        let statement = one_statement(SequenceChange::SetSequenceSchema {
            schema: "public".into(),
            name: "folios".into(),
            new_schema: "archivo".into(),
        });
        assert_eq!(
            statement.sql,
            "ALTER SEQUENCE public.folios SET SCHEMA archivo"
        );

        let statement = one_statement(SequenceChange::SetSequenceOwner {
            schema: "public".into(),
            name: "folios".into(),
            owner: "ventas".into(),
        });
        assert_eq!(
            statement.sql,
            "ALTER SEQUENCE public.folios OWNER TO ventas"
        );
    }

    #[test]
    fn borra_con_y_sin_cascade() {
        let statement = one_statement(SequenceChange::DropSequence {
            schema: "public".into(),
            name: "folios".into(),
            cascade: false,
        });
        assert_eq!(statement.sql, "DROP SEQUENCE public.folios");

        let statement = one_statement(SequenceChange::DropSequence {
            schema: "public".into(),
            name: "folios".into(),
            cascade: true,
        });
        assert_eq!(statement.sql, "DROP SEQUENCE public.folios CASCADE");
    }

    #[test]
    fn cita_los_identificadores_que_lo_necesitan() {
        let statement = one_statement(SequenceChange::CreateSequence {
            schema: "mi esquema".into(),
            name: "Folios".into(),
            if_not_exists: false,
            options: SequenceOptions {
                owned_by: Some(SequenceOwner::Column {
                    schema: "mi esquema".into(),
                    table: "Comprobantes".into(),
                    column: "folio nuevo".into(),
                }),
                ..options()
            },
        });
        assert_eq!(
            statement.sql,
            "CREATE SEQUENCE \"mi esquema\".\"Folios\" \
             OWNED BY \"mi esquema\".\"Comprobantes\".\"folio nuevo\""
        );
    }
}
