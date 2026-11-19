//! Qué es una tabla, desde el punto de vista de editarla.
//!
//! Lo único difícil de una grilla editable es responder «¿qué fila es esta fila?». La respuesta es
//! la clave: la primaria, o un índice único sobre columnas `NOT NULL`. Sin una de las dos no hay
//! forma de garantizar que un `UPDATE` toque exactamente la fila que el usuario está mirando, y la
//! grilla se abre en solo lectura.
//!
//! No se usa `ctid` como identidad de reemplazo, aunque otras herramientas lo hagan: cambia con
//! cada `UPDATE` y con un `VACUUM FULL`, así que entre que se lee la fila y se guarda el cambio
//! puede estar apuntando a otra.

use serde::{Deserialize, Serialize};

use crate::conn::ServerHandle;
use crate::ddl::quote_ident;
use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    /// Tal como lo escribe `format_type`: `character varying(20)`, `numeric(12,2)`, `public.estado`.
    /// Ya viene en una forma válida para usar en SQL, así que **no** hay que citarlo.
    pub type_name: String,
    pub not_null: bool,
    pub default: Option<String>,
    /// La calcula el servidor —identidad o columna generada—, así que no se puede escribir.
    pub generated: bool,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyKind {
    Primary,
    Unique,
}

/// Con qué columnas se identifica una fila.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    pub name: String,
    pub kind: KeyKind,
    pub columns: Vec<String>,
}

/// Una clave foránea vista desde una tabla: a qué otra tabla apunta, o qué otra tabla apunta a ella.
///
/// `columns` son siempre las de **esta** tabla y `other_columns` las de la otra, en el mismo orden,
/// así que la grilla arma el filtro del salto sin saber en qué sentido va la flecha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub name: String,
    pub columns: Vec<String>,
    pub oid: u32,
    pub schema: String,
    pub table: String,
    pub other_columns: Vec<String>,
}

/// Tope de tablas que apuntan a una: una tabla de catálogo puede tener cientos y el menú no sirve.
const MAX_REFERENCED_BY: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableShape {
    pub oid: u32,
    pub schema: String,
    pub name: String,
    pub columns: Vec<Column>,
    /// `None` cuando no hay con qué identificar una fila.
    pub key: Option<Key>,
    /// Por qué no se puede editar. `None` significa que sí se puede.
    pub read_only: Option<String>,
    /// A qué tablas apunta. `default` porque la interfaz devuelve la forma entera en cada página y
    /// una forma armada sin estos campos tiene que seguir leyéndose.
    #[serde(default)]
    pub foreign_keys: Vec<Link>,
    /// Qué tablas apuntan a esta.
    #[serde(default)]
    pub referenced_by: Vec<Link>,
}

impl TableShape {
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|column| column.name == name)
    }

    /// Las columnas que la clave usa para identificar una fila.
    pub fn key_columns(&self) -> &[String] {
        self.key.as_ref().map_or(&[], |key| &key.columns)
    }

    pub fn editable(&self) -> bool {
        self.read_only.is_none()
    }
}

const COLUMNS_SQL: &str = "
    SELECT a.attname::text,
           pg_catalog.format_type(a.atttypid, a.atttypmod),
           a.attnotnull,
           pg_catalog.pg_get_expr(d.adbin, d.adrelid),
           a.attidentity <> '' OR a.attgenerated <> '',
           pg_catalog.col_description(a.attrelid, a.attnum)
      FROM pg_catalog.pg_attribute a
      LEFT JOIN pg_catalog.pg_attrdef d
             ON d.adrelid = a.attrelid AND d.adnum = a.attnum
     WHERE a.attrelid = $1 AND a.attnum > 0 AND NOT a.attisdropped
     ORDER BY a.attnum
";

/// Índices únicos utilizables como identidad, la clave primaria primero.
///
/// Se descartan los parciales y los de expresión: un índice con `WHERE` solo garantiza unicidad
/// entre las filas que cumplen la condición, y uno sobre `lower(nombre)` no da un valor que se
/// pueda mandar de vuelta en el `WHERE` de un `UPDATE`.
///
/// `indnkeyatts` deja afuera las columnas de un `INCLUDE`, que acompañan al índice pero no
/// participan de la unicidad.
const KEY_SQL: &str = "
    SELECT c.relname::text,
           i.indisprimary,
           array_agg(a.attname::text ORDER BY k.ord),
           bool_and(a.attnotnull)
      FROM pg_catalog.pg_index i
      JOIN pg_catalog.pg_class c ON c.oid = i.indexrelid
     CROSS JOIN LATERAL unnest(i.indkey::int2[]) WITH ORDINALITY AS k(attnum, ord)
      JOIN pg_catalog.pg_attribute a
             ON a.attrelid = i.indrelid AND a.attnum = k.attnum
     WHERE i.indrelid = $1
       AND i.indisunique
       AND i.indisvalid
       AND i.indpred IS NULL
       AND i.indexprs IS NULL
       AND k.ord <= i.indnkeyatts
     GROUP BY c.relname, i.indisprimary
     ORDER BY i.indisprimary DESC, c.relname
";

/// Claves foráneas de las dos puntas. Las de una partición heredadas de su madre (`conparentid`)
/// se dejan: el mismo salto aparecería una vez por partición.
const LINKS_SQL: &str = "
    SELECT c.conname::text,
           c.conrelid::int8,
           c.confrelid::int8,
           ARRAY(SELECT a.attname::text
                   FROM unnest(c.conkey) WITH ORDINALITY AS k(attnum, ord)
                   JOIN pg_catalog.pg_attribute a
                     ON a.attrelid = c.conrelid AND a.attnum = k.attnum
                  ORDER BY k.ord),
           ARRAY(SELECT a.attname::text
                   FROM unnest(c.confkey) WITH ORDINALITY AS k(attnum, ord)
                   JOIN pg_catalog.pg_attribute a
                     ON a.attrelid = c.confrelid AND a.attnum = k.attnum
                  ORDER BY k.ord),
           n.nspname::text,
           r.relname::text
      FROM pg_catalog.pg_constraint c
      JOIN pg_catalog.pg_class r
        ON r.oid = CASE WHEN c.conrelid = $1 THEN c.confrelid ELSE c.conrelid END
      JOIN pg_catalog.pg_namespace n ON n.oid = r.relnamespace
     WHERE c.contype = 'f'
       AND c.conparentid = 0
       AND (c.conrelid = $1 OR c.confrelid = $1)
     ORDER BY n.nspname, r.relname, c.conname
";

const RELATION_SQL: &str = "
    SELECT n.nspname::text, c.relname::text, c.relkind::text
      FROM pg_catalog.pg_class c
      JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
     WHERE c.oid = $1
";

/// El oid de una relación nombrada, o un error claro si no existe.
///
/// Se resuelve con `to_regclass` y no armando el nombre a mano: es el propio servidor el que aplica
/// las reglas de citado y de mayúsculas, que es justo donde falla cualquier reconstrucción casera.
pub async fn oid_of(
    handle: &ServerHandle,
    database: &str,
    schema: &str,
    name: &str,
) -> Result<u32> {
    let client = handle.client(database).await?;
    let qualified = format!("{}.{}", quote_ident(schema), quote_ident(name));

    let row = client
        .query_one("SELECT to_regclass($1)::oid", &[&qualified])
        .await?;
    let oid: Option<u32> = row.get(0);

    oid.filter(|value| *value != 0)
        .ok_or_else(|| Error::Config(format!("no existe la relación {qualified}")))
}

/// Igual que [`shape`], pero partiendo del nombre. Lo usa la interfaz cuando lo que tiene es lo que
/// dijo un plan de ejecución —esquema y tabla— y no un oid.
pub async fn shape_by_name(
    handle: &ServerHandle,
    database: &str,
    schema: &str,
    name: &str,
) -> Result<TableShape> {
    let oid = oid_of(handle, database, schema, name).await?;
    shape(handle, database, oid).await
}

/// Lee del catálogo todo lo que hace falta para mostrar y editar una tabla.
pub async fn shape(handle: &ServerHandle, database: &str, oid: u32) -> Result<TableShape> {
    let client = handle.client(database).await?;

    let relation = client
        .query_opt(RELATION_SQL, &[&oid])
        .await?
        .ok_or_else(|| Error::Config(format!("no existe ninguna relación con el oid {oid}")))?;

    let schema: String = relation.get(0);
    let name: String = relation.get(1);
    let relkind: String = relation.get(2);

    let columns: Vec<Column> = client
        .query(COLUMNS_SQL, &[&oid])
        .await?
        .into_iter()
        .map(|row| Column {
            name: row.get(0),
            type_name: row.get(1),
            not_null: row.get(2),
            default: row.get(3),
            generated: row.get(4),
            comment: row.get(5),
        })
        .collect();

    if columns.is_empty() {
        return Err(Error::Config(format!(
            "{schema}.{name} no tiene columnas visibles para este usuario"
        )));
    }

    // Una clave única sobre una columna que admite nulos no identifica nada: en PostgreSQL dos
    // filas con NULL no se consideran iguales, así que pueden repetirse todas las veces que quieran.
    let key = client
        .query(KEY_SQL, &[&oid])
        .await?
        .into_iter()
        .find(|row| row.get::<_, bool>(3))
        .map(|row| Key {
            name: row.get(0),
            kind: if row.get(1) {
                KeyKind::Primary
            } else {
                KeyKind::Unique
            },
            columns: row.get(2),
        });

    let (foreign_keys, referenced_by) = links(&client, oid).await?;

    Ok(TableShape {
        read_only: read_only_reason(&relkind, key.as_ref()),
        foreign_keys,
        referenced_by,
        oid,
        schema,
        name,
        columns,
        key,
    })
}

/// Las claves foráneas de la tabla, separadas por sentido. Una autorreferencia cuenta en los dos.
async fn links(client: &tokio_postgres::Client, oid: u32) -> Result<(Vec<Link>, Vec<Link>)> {
    let mut outgoing = Vec::new();
    let mut incoming = Vec::new();

    for row in client.query(LINKS_SQL, &[&oid]).await? {
        let owner: i64 = row.get(1);
        let target: i64 = row.get(2);
        let own_columns: Vec<String> = row.get(3);
        let target_columns: Vec<String> = row.get(4);
        let schema: String = row.get(5);
        let table: String = row.get(6);
        let name: String = row.get(0);

        if owner == i64::from(oid) {
            outgoing.push(Link {
                name: name.clone(),
                columns: own_columns.clone(),
                oid: target as u32,
                schema: schema.clone(),
                table: table.clone(),
                other_columns: target_columns.clone(),
            });
        }
        if target == i64::from(oid) && incoming.len() < MAX_REFERENCED_BY {
            incoming.push(Link {
                name,
                columns: target_columns,
                oid: owner as u32,
                schema,
                table,
                other_columns: own_columns,
            });
        }
    }

    Ok((outgoing, incoming))
}

/// El motivo por el que la grilla se abre en solo lectura, redactado para mostrarlo tal cual.
fn read_only_reason(relkind: &str, key: Option<&Key>) -> Option<String> {
    match relkind {
        "v" => {
            return Some(
                "Es una vista. pgforge todavía no escribe a través de vistas; para cambiar los \
                 datos hay que editar las tablas de las que sale."
                    .to_owned(),
            )
        }
        "m" => {
            return Some(
                "Es una vista materializada: su contenido lo produce la consulta que la define y \
                 se reemplaza entero con REFRESH."
                    .to_owned(),
            )
        }
        "f" => {
            return Some(
                "Es una tabla foránea: los datos viven en otro servidor y pgforge todavía no los \
                 edita."
                    .to_owned(),
            )
        }
        _ => {}
    }

    key.is_none().then(|| {
        "La tabla no tiene clave primaria ni un índice único sobre columnas NOT NULL, así que no \
         hay forma de señalar una fila sin riesgo de tocar otras. Agregando una clave primaria la \
         grilla se vuelve editable."
            .to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(columns: &[&str]) -> Key {
        Key {
            name: "t_pkey".into(),
            kind: KeyKind::Primary,
            columns: columns.iter().map(|c| (*c).to_owned()).collect(),
        }
    }

    #[test]
    fn una_tabla_con_clave_es_editable() {
        assert_eq!(read_only_reason("r", Some(&key(&["id"]))), None);
        assert_eq!(read_only_reason("p", Some(&key(&["id"]))), None);
    }

    #[test]
    fn sin_clave_no_se_edita_y_se_explica_por_que() {
        let motivo = read_only_reason("r", None).expect("tiene que haber un motivo");
        assert!(
            motivo.contains("clave primaria"),
            "el motivo tiene que decir qué falta y cómo resolverlo: {motivo}"
        );
    }

    #[test]
    fn las_vistas_no_se_editan_aunque_tengan_indice_unico() {
        // Una vista materializada puede tener un índice único, así que la clave existe: si el
        // motivo se dedujera solo de la clave, se ofrecería editar algo que no se puede escribir.
        assert!(read_only_reason("m", Some(&key(&["id"]))).is_some());
        assert!(read_only_reason("v", None).is_some());
        assert!(read_only_reason("f", Some(&key(&["id"]))).is_some());
    }
}
