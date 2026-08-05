//! Grafo de relaciones de un esquema: lo que la interfaz dibuja como diagrama ERD.
//!
//! El grafo se arma acá y se posiciona en la interfaz. El layout depende del ancho del texto en
//! pantalla y de lo que el usuario arrastre, así que no es información del servidor: el core
//! devuelve tablas y aristas, nunca coordenadas.
//!
//! A diferencia del árbol, que se expande de a un nivel, esto se trae de una vez: un diagrama
//! parcial no dice nada. Son cuatro consultas acotadas al esquema.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio_postgres::Client;

use super::NodeKind;
use crate::conn::ServerHandle;
use crate::ddl::RefAction;
use crate::error::{Error, Result};

/// Las tablas de un esquema y las claves foráneas que las unen.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaGraph {
    pub database: String,
    pub schema: String,
    pub tables: Vec<GraphTable>,
    pub edges: Vec<GraphEdge>,
}

/// Una tabla del diagrama, con las columnas que se muestran adentro de la caja.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphTable {
    pub oid: u32,
    pub name: String,
    /// `Table`, `PartitionedTable` o `ForeignTable`.
    pub kind: NodeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    pub columns: Vec<GraphColumn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphColumn {
    /// `attnum`. La interfaz no lo usa para dibujar, pero sí para señalar la columna de una arista.
    pub position: i16,
    pub name: String,
    pub type_name: String,
    pub not_null: bool,
    pub primary_key: bool,
    /// Participa en alguna clave foránea saliente.
    pub foreign_key: bool,
}

/// Una clave foránea. El diagrama la dibuja como flecha de la tabla que referencia a la referida.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    /// Nombre de la restricción.
    pub name: String,
    pub source: u32,
    pub target: u32,
    pub source_columns: Vec<String>,
    pub target_columns: Vec<String>,
    pub on_update: RefAction,
    pub on_delete: RefAction,
    /// `esquema.tabla` de la referida cuando vive fuera del esquema del diagrama, es decir cuando
    /// `target` no está entre las tablas. La arista se conserva igual: esconderla mentiría sobre
    /// el modelo, y la interfaz la dibuja saliendo del borde.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_label: Option<String>,
}

/// `confupdtype`/`confdeltype` del catálogo. Lo que no está en la lista queda como `NoAction`, que
/// es lo que guarda el servidor cuando la restricción no declara ninguna acción.
fn ref_action(code: &str) -> RefAction {
    match code {
        "r" => RefAction::Restrict,
        "c" => RefAction::Cascade,
        "n" => RefAction::SetNull,
        "d" => RefAction::SetDefault,
        _ => RefAction::NoAction,
    }
}

/// Tablas del esquema y sus claves foráneas.
pub async fn schema_graph(
    handle: &ServerHandle,
    database: &str,
    schema: &str,
) -> Result<SchemaGraph> {
    let client = handle.client(database).await?;

    // Un esquema que no existe devolvería un diagrama vacío, indistinguible de uno sin tablas.
    let namespace: u32 = client
        .query_opt(
            "SELECT n.oid FROM pg_catalog.pg_namespace n WHERE n.nspname = $1",
            &[&schema],
        )
        .await?
        .map(|row| row.get(0))
        .ok_or_else(|| Error::Config(format!("el esquema «{schema}» no existe en {database}")))?;

    // Vistas y vistas materializadas quedan afuera: no participan de ninguna clave foránea, así
    // que solo agrandarían el diagrama.
    let rows = client
        .query(
            "SELECT c.oid,
                    c.relname::text,
                    c.relkind::text,
                    pg_catalog.obj_description(c.oid, 'pg_class')
               FROM pg_catalog.pg_class c
              WHERE c.relnamespace = $1 AND c.relkind IN ('r', 'p', 'f')
              ORDER BY c.relname",
            &[&namespace],
        )
        .await?;

    let mut tables: Vec<GraphTable> = rows
        .into_iter()
        .map(|row| {
            let relkind: String = row.get(2);
            GraphTable {
                oid: row.get(0),
                name: row.get(1),
                kind: match relkind.as_str() {
                    "p" => NodeKind::PartitionedTable,
                    "f" => NodeKind::ForeignTable,
                    _ => NodeKind::Table,
                },
                comment: row.get(3),
                columns: Vec::new(),
            }
        })
        .collect();

    let oids: Vec<u32> = tables.iter().map(|table| table.oid).collect();
    if oids.is_empty() {
        return Ok(SchemaGraph {
            database: database.to_owned(),
            schema: schema.to_owned(),
            tables,
            edges: Vec::new(),
        });
    }

    // `conparentid = 0` deja afuera las copias que cada partición hereda de su padre: sin ese
    // filtro una tabla particionada dibuja la misma arista tantas veces como particiones tenga.
    let rows = client
        .query(
            "SELECT con.conname::text,
                    con.conrelid,
                    con.confrelid,
                    con.conkey,
                    con.confkey,
                    con.confupdtype::text,
                    con.confdeltype::text,
                    tn.nspname::text,
                    tc.relname::text
               FROM pg_catalog.pg_constraint con
               JOIN pg_catalog.pg_class tc ON tc.oid = con.confrelid
               JOIN pg_catalog.pg_namespace tn ON tn.oid = tc.relnamespace
              WHERE con.contype = 'f'
                AND con.conrelid = ANY($1::oid[])
                AND con.conparentid = 0
              ORDER BY con.conrelid, con.conname",
            &[&oids],
        )
        .await?;

    // Las columnas se piden también de las tablas referidas de otros esquemas: sin ellas la
    // arista no podría decir contra qué columna referencia.
    let mut column_oids = oids.clone();
    for row in &rows {
        let target: u32 = row.get(2);
        if !column_oids.contains(&target) {
            column_oids.push(target);
        }
    }

    let columns = columns_by_relation(&client, &column_oids).await?;

    let mut edges: Vec<GraphEdge> = rows
        .into_iter()
        .map(|row| {
            let source: u32 = row.get(1);
            let target: u32 = row.get(2);
            let source_keys: Vec<i16> = row.get(3);
            let target_keys: Vec<i16> = row.get(4);
            let update_code: String = row.get(5);
            let delete_code: String = row.get(6);
            let target_schema: String = row.get(7);
            let target_name: String = row.get(8);

            GraphEdge {
                name: row.get(0),
                source,
                target,
                source_columns: column_names(&columns, source, &source_keys),
                target_columns: column_names(&columns, target, &target_keys),
                on_update: ref_action(&update_code),
                on_delete: ref_action(&delete_code),
                target_label: (!oids.contains(&target))
                    .then(|| format!("{target_schema}.{target_name}")),
            }
        })
        .collect();

    for table in &mut tables {
        let mut own = columns.get(&table.oid).cloned().unwrap_or_default();
        for column in &mut own {
            column.foreign_key = edges
                .iter()
                .any(|edge| edge.source == table.oid && edge.source_columns.contains(&column.name));
        }
        table.columns = own;
    }

    // Orden estable: el diagrama se posiciona a partir de esta lista y no puede moverse solo
    // porque el servidor devolvió las filas en otro orden.
    edges.sort_by(|a, b| (a.source, &a.name).cmp(&(b.source, &b.name)));

    Ok(SchemaGraph {
        database: database.to_owned(),
        schema: schema.to_owned(),
        tables,
        edges,
    })
}

async fn columns_by_relation(
    client: &Client,
    oids: &[u32],
) -> Result<HashMap<u32, Vec<GraphColumn>>> {
    let rows = client
        .query(
            "SELECT a.attrelid,
                    a.attnum,
                    a.attname::text,
                    pg_catalog.format_type(a.atttypid, a.atttypmod),
                    a.attnotnull,
                    COALESCE(i.indisprimary, false)
               FROM pg_catalog.pg_attribute a
               LEFT JOIN pg_catalog.pg_index i
                      ON i.indrelid = a.attrelid AND i.indisprimary AND a.attnum = ANY(i.indkey)
              WHERE a.attrelid = ANY($1::oid[]) AND a.attnum > 0 AND NOT a.attisdropped
              ORDER BY a.attrelid, a.attnum",
            &[&oids],
        )
        .await?;

    let mut by_relation: HashMap<u32, Vec<GraphColumn>> = HashMap::new();
    for row in rows {
        by_relation
            .entry(row.get(0))
            .or_default()
            .push(GraphColumn {
                position: row.get(1),
                name: row.get(2),
                type_name: row.get(3),
                not_null: row.get(4),
                primary_key: row.get(5),
                foreign_key: false,
            });
    }

    Ok(by_relation)
}

/// Nombres de las columnas de `relation` en el orden en que los nombra la restricción, que no es
/// necesariamente el de la tabla.
fn column_names(
    columns: &HashMap<u32, Vec<GraphColumn>>,
    relation: u32,
    keys: &[i16],
) -> Vec<String> {
    let Some(columns) = columns.get(&relation) else {
        return Vec::new();
    };

    keys.iter()
        .filter_map(|key| {
            columns
                .iter()
                .find(|column| column.position == *key)
                .map(|column| column.name.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traduce_los_codigos_de_accion_referencial() {
        assert_eq!(ref_action("c"), RefAction::Cascade);
        assert_eq!(ref_action("n"), RefAction::SetNull);
        // El código en blanco es lo que guarda el servidor cuando no se escribió ninguna acción.
        assert_eq!(ref_action(" "), RefAction::NoAction);
    }

    #[test]
    fn los_nombres_de_columna_siguen_el_orden_de_la_restriccion() {
        let mut columns = HashMap::new();
        columns.insert(
            1_u32,
            vec![column(1, "empresa"), column(2, "sucursal"), column(3, "id")],
        );

        // La restricción nombra (sucursal, empresa), al revés que la tabla.
        assert_eq!(
            column_names(&columns, 1, &[2, 1]),
            vec!["sucursal".to_owned(), "empresa".to_owned()]
        );
    }

    fn column(position: i16, name: &str) -> GraphColumn {
        GraphColumn {
            position,
            name: name.to_owned(),
            type_name: "text".to_owned(),
            not_null: false,
            primary_key: false,
            foreign_key: false,
        }
    }
}
