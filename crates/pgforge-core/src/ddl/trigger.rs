//! Crear, cambiar y borrar triggers.
//!
//! No hay `CREATE OR REPLACE TRIGGER` en el piso soportado: existe desde PostgreSQL 14 y pgforge
//! soporta 13+. Cambiar un trigger es borrarlo y crearlo de nuevo, mismo criterio que ya usa
//! [`crate::ddl::view`] para una vista materializada — con la diferencia de que acá no hace falta
//! conservar el nombre: nada más referencia a un trigger por nombre, así que el nombre nuevo puede
//! ser distinto del viejo sin ningún problema.
//!
//! La condición `WHEN` es SQL crudo, misma frontera de confianza que un `CHECK` o el `default` de
//! una columna.

use serde::{Deserialize, Serialize};

use crate::conn::ServerHandle;
use crate::error::{Error, Result};

use super::table::Statement;
use super::{qualified, quote_ident};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Timing {
    Before,
    After,
    InsteadOf,
}

impl Timing {
    fn sql(self) -> &'static str {
        match self {
            Timing::Before => "BEFORE",
            Timing::After => "AFTER",
            Timing::InsteadOf => "INSTEAD OF",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    Insert,
    Update,
    Delete,
    Truncate,
}

impl Event {
    fn sql(self) -> &'static str {
        match self {
            Event::Insert => "INSERT",
            Event::Update => "UPDATE",
            Event::Delete => "DELETE",
            Event::Truncate => "TRUNCATE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Row,
    Statement,
}

impl Level {
    fn sql(self) -> &'static str {
        match self {
            Level::Row => "ROW",
            Level::Statement => "STATEMENT",
        }
    }
}

/// Lo que define un trigger nuevo.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerDef {
    pub timing: Timing,
    pub events: Vec<Event>,
    pub level: Level,
    pub when: Option<String>,
    pub function_schema: String,
    /// Se llama sin argumentos: `EXECUTE FUNCTION esquema.nombre()`.
    pub function_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TriggerChange {
    CreateTrigger {
        schema: String,
        table: String,
        name: String,
        definition: TriggerDef,
    },
    DropTrigger {
        schema: String,
        table: String,
        name: String,
        cascade: bool,
    },
}

fn statement(sql: String) -> Statement {
    Statement { sql }
}

pub fn statements(changes: &[TriggerChange]) -> Result<Vec<Statement>> {
    changes.iter().map(one).collect()
}

fn one(change: &TriggerChange) -> Result<Statement> {
    match change {
        TriggerChange::CreateTrigger {
            schema,
            table,
            name,
            definition,
        } => create_trigger(schema, table, name, definition),
        TriggerChange::DropTrigger {
            schema,
            table,
            name,
            cascade,
        } => Ok(statement(format!(
            "DROP TRIGGER {} ON {}{}",
            quote_ident(name),
            qualified(schema, table),
            if *cascade { " CASCADE" } else { "" }
        ))),
    }
}

fn create_trigger(schema: &str, table: &str, name: &str, def: &TriggerDef) -> Result<Statement> {
    if def.events.is_empty() {
        return Err(Error::Config(
            "un trigger necesita al menos un evento".to_owned(),
        ));
    }
    if def.function_name.trim().is_empty() {
        return Err(Error::Config(
            "un trigger necesita la función que va a ejecutar".to_owned(),
        ));
    }

    let events = def
        .events
        .iter()
        .map(|event| event.sql())
        .collect::<Vec<_>>()
        .join(" OR ");

    let mut sql = format!(
        "CREATE TRIGGER {}\n    {} {}\n    ON {}\n    FOR EACH {}",
        quote_ident(name),
        def.timing.sql(),
        events,
        qualified(schema, table),
        def.level.sql()
    );

    if let Some(when) = &def.when {
        if !when.trim().is_empty() {
            sql.push_str(&format!("\n    WHEN ({when})"));
        }
    }

    sql.push_str(&format!(
        "\n    EXECUTE FUNCTION {}()",
        qualified(&def.function_schema, &def.function_name)
    ));

    Ok(statement(sql))
}

/// Aplica los cambios en una sola transacción: mismo molde que `table::apply`.
pub async fn apply(handle: &ServerHandle, database: &str, changes: &[TriggerChange]) -> Result<()> {
    let statements = statements(changes)?;
    let mut client = handle.client(database).await?;
    let transaction = client.transaction().await?;

    for statement in &statements {
        transaction.batch_execute(&statement.sql).await?;
    }

    transaction.commit().await?;
    Ok(())
}

/// Un trigger tal como ya existe, para mostrarlo y ofrecer editarlo o borrarlo.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerInfo {
    pub oid: u32,
    pub name: String,
    pub timing: Timing,
    pub events: Vec<Event>,
    pub level: Level,
    pub when: Option<String>,
    pub function_schema: String,
    pub function_name: String,
}

// Bits de `pg_trigger.tgtype`, estables en el catálogo de Postgres desde hace muchas versiones. No
// hay una función que ya los separe — `pg_get_triggerdef` arma el DDL completo, no sus partes.
const TRIGGER_TYPE_ROW: i16 = 1 << 0;
const TRIGGER_TYPE_BEFORE: i16 = 1 << 1;
const TRIGGER_TYPE_INSERT: i16 = 1 << 2;
const TRIGGER_TYPE_DELETE: i16 = 1 << 3;
const TRIGGER_TYPE_UPDATE: i16 = 1 << 4;
const TRIGGER_TYPE_TRUNCATE: i16 = 1 << 5;
const TRIGGER_TYPE_INSTEAD: i16 = 1 << 6;

const TRIGGERS_SQL: &str = "
    SELECT t.oid,
           t.tgname::text,
           t.tgtype,
           pg_catalog.pg_get_expr(t.tgqual, t.tgrelid),
           p.proname::text,
           n2.nspname::text
      FROM pg_catalog.pg_trigger t
      JOIN pg_catalog.pg_proc p ON p.oid = t.tgfoid
      JOIN pg_catalog.pg_namespace n2 ON n2.oid = p.pronamespace
     WHERE t.tgrelid = $1 AND NOT t.tgisinternal
     ORDER BY t.tgname
";

/// Los triggers que ya tiene una tabla. Mismo filtro (`NOT tgisinternal`) que ya usa
/// `introspect::triggers` para no traer los que arma Postgres solo detrás de una constraint.
pub async fn triggers(handle: &ServerHandle, database: &str, oid: u32) -> Result<Vec<TriggerInfo>> {
    let client = handle.client(database).await?;
    let rows = client.query(TRIGGERS_SQL, &[&oid]).await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let tgtype: i16 = row.get(2);

            let timing = if tgtype & TRIGGER_TYPE_BEFORE != 0 {
                Timing::Before
            } else if tgtype & TRIGGER_TYPE_INSTEAD != 0 {
                Timing::InsteadOf
            } else {
                Timing::After
            };

            let mut events = Vec::new();
            if tgtype & TRIGGER_TYPE_INSERT != 0 {
                events.push(Event::Insert);
            }
            if tgtype & TRIGGER_TYPE_UPDATE != 0 {
                events.push(Event::Update);
            }
            if tgtype & TRIGGER_TYPE_DELETE != 0 {
                events.push(Event::Delete);
            }
            if tgtype & TRIGGER_TYPE_TRUNCATE != 0 {
                events.push(Event::Truncate);
            }

            let level = if tgtype & TRIGGER_TYPE_ROW != 0 {
                Level::Row
            } else {
                Level::Statement
            };

            TriggerInfo {
                oid: row.get(0),
                name: row.get(1),
                timing,
                events,
                level,
                when: row.get(3),
                function_name: row.get(4),
                function_schema: row.get(5),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> TriggerDef {
        TriggerDef {
            timing: Timing::Before,
            events: vec![Event::Insert],
            level: Level::Row,
            when: None,
            function_schema: "public".into(),
            function_name: "auditar".into(),
        }
    }

    fn one_statement(change: TriggerChange) -> Statement {
        statements(&[change])
            .expect("tenía que generar la sentencia")
            .remove(0)
    }

    #[test]
    fn crea_un_trigger_simple() {
        let statement = one_statement(TriggerChange::CreateTrigger {
            schema: "public".into(),
            table: "clientes".into(),
            name: "clientes_bi".into(),
            definition: def(),
        });
        assert_eq!(
            statement.sql,
            "CREATE TRIGGER clientes_bi\n    \
             BEFORE INSERT\n    \
             ON public.clientes\n    \
             FOR EACH ROW\n    \
             EXECUTE FUNCTION public.auditar()"
        );
    }

    #[test]
    fn combina_varios_eventos_con_or() {
        let mut d = def();
        d.events = vec![Event::Insert, Event::Update, Event::Delete];
        let statement = one_statement(TriggerChange::CreateTrigger {
            schema: "public".into(),
            table: "clientes".into(),
            name: "clientes_bi".into(),
            definition: d,
        });
        assert!(
            statement
                .sql
                .contains("BEFORE INSERT OR UPDATE OR DELETE\n"),
            "{}",
            statement.sql
        );
    }

    #[test]
    fn cada_momento_se_traduce() {
        for (timing, palabra) in [
            (Timing::Before, "BEFORE"),
            (Timing::After, "AFTER"),
            (Timing::InsteadOf, "INSTEAD OF"),
        ] {
            let mut d = def();
            d.timing = timing;
            let statement = one_statement(TriggerChange::CreateTrigger {
                schema: "public".into(),
                table: "clientes".into(),
                name: "t".into(),
                definition: d,
            });
            assert!(
                statement.sql.contains(&format!("\n    {palabra} INSERT\n")),
                "{}: {}",
                palabra,
                statement.sql
            );
        }
    }

    #[test]
    fn agrega_la_condicion_when() {
        let mut d = def();
        d.when = Some("NEW.activo".into());
        let statement = one_statement(TriggerChange::CreateTrigger {
            schema: "public".into(),
            table: "clientes".into(),
            name: "t".into(),
            definition: d,
        });
        assert!(
            statement.sql.contains("\n    WHEN (NEW.activo)\n"),
            "{}",
            statement.sql
        );
    }

    #[test]
    fn sin_when_no_aparece_la_clausula() {
        let statement = one_statement(TriggerChange::CreateTrigger {
            schema: "public".into(),
            table: "clientes".into(),
            name: "t".into(),
            definition: def(),
        });
        assert!(!statement.sql.contains("WHEN"), "{}", statement.sql);
    }

    #[test]
    fn un_trigger_sin_eventos_no_se_genera() {
        let mut d = def();
        d.events = vec![];
        assert!(statements(&[TriggerChange::CreateTrigger {
            schema: "public".into(),
            table: "clientes".into(),
            name: "t".into(),
            definition: d,
        }])
        .is_err());
    }

    #[test]
    fn un_trigger_sin_funcion_no_se_genera() {
        let mut d = def();
        d.function_name = "  ".into();
        assert!(statements(&[TriggerChange::CreateTrigger {
            schema: "public".into(),
            table: "clientes".into(),
            name: "t".into(),
            definition: d,
        }])
        .is_err());
    }

    #[test]
    fn borra_un_trigger_con_y_sin_cascade() {
        let statement = one_statement(TriggerChange::DropTrigger {
            schema: "public".into(),
            table: "clientes".into(),
            name: "clientes_bi".into(),
            cascade: false,
        });
        assert_eq!(statement.sql, "DROP TRIGGER clientes_bi ON public.clientes");

        let statement = one_statement(TriggerChange::DropTrigger {
            schema: "public".into(),
            table: "clientes".into(),
            name: "clientes_bi".into(),
            cascade: true,
        });
        assert_eq!(
            statement.sql,
            "DROP TRIGGER clientes_bi ON public.clientes CASCADE"
        );
    }

    #[test]
    fn cita_los_identificadores_que_lo_necesitan() {
        let mut d = def();
        d.function_schema = "mi esquema".into();
        d.function_name = "Auditar".into();
        let statement = one_statement(TriggerChange::CreateTrigger {
            schema: "public".into(),
            table: "clientes".into(),
            name: "Mi Trigger".into(),
            definition: d,
        });
        assert!(
            statement.sql.contains("CREATE TRIGGER \"Mi Trigger\""),
            "{}",
            statement.sql
        );
        assert!(
            statement
                .sql
                .contains("EXECUTE FUNCTION \"mi esquema\".\"Auditar\"()"),
            "{}",
            statement.sql
        );
    }
}
