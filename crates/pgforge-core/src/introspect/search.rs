//! Búsqueda de objetos por nombre dentro de una base.
//!
//! El árbol se carga por niveles, así que buscar sobre lo que ya se trajo solo encuentra lo que uno
//! ya había abierto —justo lo que no hace falta buscar—. Acá la pregunta va al servidor: una sola
//! consulta al catálogo de la base, sin recorrer esquema por esquema.
//!
//! Se buscan relaciones, rutinas y tipos. Columnas, índices, restricciones y disparadores quedan
//! afuera a propósito: `pg_attribute` tiene un orden de magnitud más filas que `pg_class` y un
//! resultado con doscientas columnas llamadas `id` no ayuda a encontrar nada.

use serde::{Deserialize, Serialize};

use super::{NodeKind, TreeOptions};
use crate::conn::ServerHandle;
use crate::error::Result;

/// Una coincidencia. Lleva lo justo para que la interfaz pueda abrir el camino hasta el objeto:
/// esquema, OID y de qué tipo es —el tipo dice en qué carpeta del árbol vive—.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub kind: NodeKind,
    pub database: String,
    pub schema: String,
    /// El nombre como se muestra: en una rutina incluye la firma, porque el nombre solo no
    /// distingue las sobrecargas.
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub oid: u32,
}

/// Los esquemas que se saltean cuando no se piden los del sistema. Es el mismo predicado que usa
/// `schemas()`: si acá se buscara en `pg_catalog`, el resultado traería objetos que el árbol no
/// muestra y que después no se pueden revelar.
const VISIBLE_SCHEMAS: &str = "($1 OR NOT (n.nspname IN ('pg_catalog', 'information_schema')
                                           OR n.nspname LIKE 'pg\\_toast%'
                                           OR n.nspname LIKE 'pg\\_temp%'))";

/// Coincidencia por subcadena, sin distinguir mayúsculas.
///
/// Es `strpos` y no `ILIKE '%…%'` para no tener que escapar `%` ni `_`: un patrón escrito por el
/// usuario con un guión bajo —que en los nombres de tabla hay en todos— pasaría a ser un comodín.
/// De paso, la misma posición sirve después para ordenar.
fn build_sql() -> String {
    format!(
        "SELECT * FROM (
             SELECT 'rel'::text AS src,
                    c.oid AS oid,
                    n.nspname::text AS schema,
                    c.relname::text AS name,
                    c.relkind::text AS code,
                    NULL::text AS extra
               FROM pg_catalog.pg_class c
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
              WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f', 'S')
                AND {VISIBLE_SCHEMAS}
                AND strpos(lower(c.relname), lower($2)) > 0
             UNION ALL
             SELECT 'proc',
                    p.oid,
                    n.nspname::text,
                    p.proname::text,
                    p.prokind::text,
                    pg_catalog.pg_get_function_identity_arguments(p.oid)
               FROM pg_catalog.pg_proc p
               JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
              WHERE p.prokind IN ('f', 'w', 'p')
                AND {VISIBLE_SCHEMAS}
                AND strpos(lower(p.proname), lower($2)) > 0
             UNION ALL
             SELECT 'type',
                    t.oid,
                    n.nspname::text,
                    t.typname::text,
                    t.typtype::text,
                    NULL
               FROM pg_catalog.pg_type t
               JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
              WHERE (t.typrelid = 0
                     OR (SELECT c.relkind FROM pg_catalog.pg_class c WHERE c.oid = t.typrelid) = 'c')
                AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_type el
                                 WHERE el.oid = t.typelem AND el.typarray = t.oid)
                AND {VISIBLE_SCHEMAS}
                AND strpos(lower(t.typname), lower($2)) > 0
         ) hit
          ORDER BY lower(hit.name) = lower($2) DESC,
                   strpos(lower(hit.name), lower($2)),
                   hit.name,
                   hit.schema
          LIMIT $3"
    )
}

/// Busca objetos cuyo nombre contenga `pattern` en una base.
///
/// Un patrón vacío devuelve nada en vez del catálogo entero: con dos letras ya hay resultado útil,
/// y sin ninguna lo que se pediría es «traeme todo», que es justamente lo que el árbol evita.
pub async fn search(
    handle: &ServerHandle,
    database: &str,
    pattern: &str,
    options: TreeOptions,
    limit: i64,
) -> Result<Vec<SearchHit>> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Ok(Vec::new());
    }

    let client = handle.client(database).await?;
    let rows = client
        .query(
            build_sql().as_str(),
            &[&options.show_system_schemas, &pattern, &limit],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let src: String = row.get(0);
            let code: String = row.get(4);
            let name: String = row.get(3);
            let extra: Option<String> = row.get(5);

            let (kind, label, detail) = match src.as_str() {
                "rel" => (relation_kind(&code), name, None),
                "proc" => {
                    let kind = if code == "p" {
                        NodeKind::Procedure
                    } else {
                        NodeKind::Function
                    };
                    // Misma etiqueta que en el árbol, o el resultado y el nodo revelado se llamarían
                    // distinto.
                    (kind, format!("{name}({})", extra.unwrap_or_default()), None)
                }
                _ => (NodeKind::Type, name, Some(type_detail(&code).to_owned())),
            };

            SearchHit {
                kind,
                database: database.to_owned(),
                schema: row.get(2),
                label,
                detail,
                oid: row.get(1),
            }
        })
        .collect())
}

fn relation_kind(relkind: &str) -> NodeKind {
    match relkind {
        "p" => NodeKind::PartitionedTable,
        "v" => NodeKind::View,
        "m" => NodeKind::MaterializedView,
        "f" => NodeKind::ForeignTable,
        "S" => NodeKind::Sequence,
        _ => NodeKind::Table,
    }
}

fn type_detail(typtype: &str) -> &'static str {
    match typtype {
        "e" => "enumerado",
        "c" => "compuesto",
        "d" => "dominio",
        "r" => "rango",
        "m" => "multirango",
        "b" => "base",
        _ => "tipo",
    }
}
