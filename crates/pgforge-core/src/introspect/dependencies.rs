//! De qué depende una relación y qué depende de ella.
//!
//! Es la pregunta de antes de borrar o cambiar una tabla: qué vistas dejan de andar y qué claves
//! foráneas apuntan acá. Las dos mitades salen del catálogo: las claves foráneas de `pg_constraint`
//! y las vistas de `pg_depend`, que las une a sus tablas a través de la regla `_RETURN` de
//! `pg_rewrite`. Una vista no depende de la tabla directamente, depende de su propia regla.

use serde::Serialize;

use crate::conn::ServerHandle;
use crate::error::Result;

/// Hacia dónde apunta la dependencia respecto de la relación consultada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// La relación necesita a este objeto (una clave foránea que sale, una tabla que lee una vista).
    Uses,
    /// Este objeto necesita a la relación (una clave foránea que llega, una vista que la lee).
    UsedBy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub direction: Direction,
    /// Etiqueta para mostrar («tabla», «vista», «clave foránea»…), no un valor para reinterpretar.
    pub kind: String,
    pub schema: String,
    pub name: String,
    /// Lo que no entra en el nombre: la definición de una clave foránea. `None` en el resto.
    pub detail: Option<String>,
}

// `UNION` y no `UNION ALL`: una vista que lee tres columnas de la misma tabla deja tres filas en
// `pg_depend`, y la lista tiene que decirlo una vez. El `ORDER BY` por posición alcanza porque las
// cuatro primeras columnas son las mismas en las cuatro ramas.
const DEPENDENCIES_SQL: &str = "
    SELECT 'usedBy', c.relkind::text, n.nspname::text, c.relname::text, NULL::text
      FROM pg_catalog.pg_depend d
      JOIN pg_catalog.pg_rewrite r ON d.classid = 'pg_catalog.pg_rewrite'::regclass AND r.oid = d.objid
      JOIN pg_catalog.pg_class c ON c.oid = r.ev_class
      JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
     WHERE d.refclassid = 'pg_catalog.pg_class'::regclass
       AND d.refobjid = $1
       AND c.oid <> $1
    UNION
    SELECT 'usedBy', 'FK', n.nspname::text, c.relname::text,
           con.conname::text || ': ' || pg_catalog.pg_get_constraintdef(con.oid, true)
      FROM pg_catalog.pg_constraint con
      JOIN pg_catalog.pg_class c ON c.oid = con.conrelid
      JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
     WHERE con.contype = 'f' AND con.confrelid = $1 AND con.conrelid <> $1
    UNION
    SELECT 'uses', c.relkind::text, n.nspname::text, c.relname::text, NULL::text
      FROM pg_catalog.pg_depend d
      JOIN pg_catalog.pg_rewrite r ON d.classid = 'pg_catalog.pg_rewrite'::regclass AND r.oid = d.objid
      JOIN pg_catalog.pg_class c ON c.oid = d.refobjid
      JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
     WHERE d.refclassid = 'pg_catalog.pg_class'::regclass
       AND r.ev_class = $1
       AND d.refobjid <> $1
    UNION
    SELECT 'uses', 'FK', n.nspname::text, c.relname::text,
           con.conname::text || ': ' || pg_catalog.pg_get_constraintdef(con.oid, true)
      FROM pg_catalog.pg_constraint con
      JOIN pg_catalog.pg_class c ON c.oid = con.confrelid
      JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
     WHERE con.contype = 'f' AND con.conrelid = $1 AND con.confrelid <> $1
    ORDER BY 1 DESC, 3, 4, 2
";

fn kind_label(code: &str) -> &'static str {
    match code {
        "FK" => "clave foránea",
        "r" => "tabla",
        "p" => "tabla particionada",
        "v" => "vista",
        "m" => "vista materializada",
        "f" => "tabla externa",
        "S" => "secuencia",
        "i" | "I" => "índice",
        _ => "relación",
    }
}

/// Las dependencias de una relación, las que salen primero (`uses`) y después las que llegan.
///
/// Las autorreferencias (una tabla con una clave foránea a sí misma) se dejan afuera: no informan de
/// nada que ya no diga la propia lista de restricciones.
pub async fn dependencies(
    handle: &ServerHandle,
    database: &str,
    oid: u32,
) -> Result<Vec<Dependency>> {
    let client = handle.client(database).await?;
    let rows = client.query(DEPENDENCIES_SQL, &[&oid]).await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let direction: String = row.get(0);
            let code: String = row.get(1);
            Dependency {
                direction: if direction == "uses" {
                    Direction::Uses
                } else {
                    Direction::UsedBy
                },
                kind: kind_label(&code).to_owned(),
                schema: row.get(2),
                name: row.get(3),
                detail: row.get(4),
            }
        })
        .collect())
}
