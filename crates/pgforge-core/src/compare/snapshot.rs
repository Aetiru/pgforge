//! Lo que hay en un esquema, leído del catálogo.
//!
//! Es la única parte de [`crate::compare`] que habla con el servidor: todo lo demás trabaja sobre
//! estas estructuras, sin red y sin PostgreSQL. Por eso acá no se compara ni se genera nada — solo
//! se lee.
//!
//! Dos decisiones que sacan ruido de la comparación antes de que exista:
//!
//! - Las **particiones** no se leen como tablas sueltas. Cuelgan de su madre y comparten su forma,
//!   así que una columna que falta aparecería repetida una vez por partición en vez de una vez.
//! - Las **secuencias de una columna** (`serial` o `identity`) tampoco. Son parte de la definición
//!   de esa columna, y ya se comparan con ella; listarlas aparte sería contar dos veces lo mismo.
//!
//! Tampoco se leen datos: `last_value` de una secuencia es estado, no estructura, y traerlo haría
//! que dos esquemas idénticos aparezcan distintos por el solo hecho de que uno se usó.

use std::collections::HashMap;

use crate::conn::ServerHandle;
use crate::ddl::table::Identity;
use crate::error::{Error, Result};
use crate::ServerVersion;

/// Un esquema entero, tal como estaba cuando se leyó.
#[derive(Debug, Clone)]
pub struct SchemaSnapshot {
    pub database: String,
    pub schema: String,
    pub version: ServerVersion,
    /// Todos los esquemas de la base, no solo el comparado. Los usa la comparación de vistas para
    /// distinguir un `esquema.tabla` de un `tabla.columna` sin analizar el SQL.
    pub schemas: Vec<String>,
    pub tables: Vec<Table>,
    pub views: Vec<View>,
    pub sequences: Vec<Sequence>,
    pub types: Vec<TypeDef>,
}

/// Un objeto cuyo texto ya lo escribe el servidor: una restricción o un índice.
///
/// Se guarda la definición tal cual la devuelve `pg_get_constraintdef` / `pg_get_indexdef` en vez de
/// desarmarla en campos: es la que el propio PostgreSQL considera correcta, y volver a armarla a
/// mano solo agrega maneras de equivocarse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedDef {
    pub name: String,
    pub definition: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    Ordinary,
    Partitioned,
    Foreign,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    /// Como lo escribe `format_type`, que es como se escribiría en un `CREATE TABLE`.
    pub type_name: String,
    pub not_null: bool,
    pub default: Option<String>,
    pub identity: Option<Identity>,
    /// Expresión de una columna generada. Va aparte del `default` porque no se cambia igual: el
    /// `DEFAULT` se pisa con un `SET DEFAULT` y esto no se puede alterar.
    pub generated: Option<String>,
    /// Solo cuando no es la del tipo: repetirla en cada columna de texto no dice nada.
    pub collation: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub kind: RelationKind,
    /// `PARTITION BY …` de una tabla particionada.
    pub partition_by: Option<String>,
    pub columns: Vec<Column>,
    pub constraints: Vec<NamedDef>,
    /// Solo los índices que no respaldan una restricción: los de una clave primaria o única ya se
    /// comparan como restricción, y listarlos también acá mostraría cada diferencia dos veces.
    pub indexes: Vec<NamedDef>,
}

#[derive(Debug, Clone)]
pub struct View {
    pub name: String,
    pub materialized: bool,
    pub definition: String,
    /// Una vista materializada sí puede tener índices propios.
    pub indexes: Vec<NamedDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sequence {
    pub name: String,
    pub type_name: String,
    pub start: i64,
    pub increment: i64,
    pub min_value: i64,
    pub max_value: i64,
    pub cache: i64,
    pub cycle: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Enum,
    Composite,
    Domain,
    Range,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone)]
pub struct TypeDef {
    pub name: String,
    pub kind: TypeKind,
    /// Valores de una enumeración, en su orden.
    pub labels: Vec<String>,
    /// Campos de un tipo compuesto.
    pub fields: Vec<Field>,
    /// Tipo base de un dominio, o subtipo de un rango.
    pub base: Option<String>,
    pub not_null: bool,
    pub default: Option<String>,
    /// `CHECK` de un dominio.
    pub checks: Vec<NamedDef>,
}

/// Lee el esquema entero de una base.
///
/// Son varias consultas cortas contra `pg_catalog` y no una sola con todo adentro: cada objeto
/// tiene su propio catálogo y unirlos en una consulta gigante solo la haría ilegible.
pub async fn read(handle: &ServerHandle, database: &str, schema: &str) -> Result<SchemaSnapshot> {
    let client = handle.client(database).await?;

    let exists = client
        .query_opt(
            "SELECT 1 FROM pg_catalog.pg_namespace WHERE nspname = $1",
            &[&schema],
        )
        .await?;
    if exists.is_none() {
        return Err(Error::Config(format!(
            "el esquema «{schema}» no existe en la base «{database}»"
        )));
    }

    // Tablas y vistas salen de la misma consulta: son el mismo catálogo y se distinguen por
    // `relkind`. Los índices y las columnas se piden después, para todo el esquema de una vez, y se
    // reparten por OID: una consulta por tabla contra un esquema con cientos sería una tormenta de
    // idas y vueltas.
    let rows = client
        .query(
            "SELECT c.oid,
                    c.relname::text,
                    c.relkind::text,
                    CASE WHEN c.relkind = 'p'
                         THEN pg_catalog.pg_get_partkeydef(c.oid) END,
                    CASE WHEN c.relkind IN ('v', 'm')
                         THEN pg_catalog.pg_get_viewdef(c.oid, true) END
               FROM pg_catalog.pg_class c
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1
                AND c.relkind IN ('r', 'p', 'f', 'v', 'm')
                AND NOT c.relispartition
              ORDER BY c.relname",
            &[&schema],
        )
        .await?;

    let mut columns = columns(&client, schema).await?;
    let mut constraints = constraints(&client, schema).await?;
    let mut indexes = indexes(&client, schema).await?;

    let mut tables = Vec::new();
    let mut views = Vec::new();
    for row in &rows {
        let oid: u32 = row.get(0);
        let name: String = row.get(1);
        let relkind: String = row.get(2);

        match relkind.as_str() {
            "v" | "m" => views.push(View {
                name,
                materialized: relkind == "m",
                definition: row.get::<_, Option<String>>(4).unwrap_or_default(),
                indexes: indexes.remove(&oid).unwrap_or_default(),
            }),
            other => tables.push(Table {
                name,
                kind: match other {
                    "p" => RelationKind::Partitioned,
                    "f" => RelationKind::Foreign,
                    _ => RelationKind::Ordinary,
                },
                partition_by: row.get(3),
                columns: columns.remove(&oid).unwrap_or_default(),
                constraints: constraints.remove(&oid).unwrap_or_default(),
                indexes: indexes.remove(&oid).unwrap_or_default(),
            }),
        }
    }

    Ok(SchemaSnapshot {
        database: database.to_owned(),
        schema: schema.to_owned(),
        version: handle.caps.version,
        schemas: schema_names(&client).await?,
        tables,
        views,
        sequences: sequences(&client, schema).await?,
        types: types(&client, schema).await?,
    })
}

/// Los nombres de todos los esquemas de la base, del sistema incluidos: una vista puede nombrar
/// `pg_catalog.pg_class` igual que cualquier otra tabla.
async fn schema_names(client: &tokio_postgres::Client) -> Result<Vec<String>> {
    let rows = client
        .query("SELECT n.nspname::text FROM pg_catalog.pg_namespace n", &[])
        .await?;
    Ok(rows.iter().map(|row| row.get(0)).collect())
}

async fn columns(
    client: &tokio_postgres::Client,
    schema: &str,
) -> Result<HashMap<u32, Vec<Column>>> {
    let rows = client
        .query(
            "SELECT a.attrelid,
                    a.attname::text,
                    pg_catalog.format_type(a.atttypid, a.atttypmod),
                    a.attnotnull,
                    pg_catalog.pg_get_expr(d.adbin, d.adrelid),
                    a.attidentity::text,
                    a.attgenerated::text,
                    co.collname::text
               FROM pg_catalog.pg_attribute a
               JOIN pg_catalog.pg_class c ON c.oid = a.attrelid
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
               LEFT JOIN pg_catalog.pg_attrdef d
                      ON d.adrelid = a.attrelid AND d.adnum = a.attnum
               -- La colación solo se anota cuando difiere de la del tipo: si no, cada columna de
               -- texto arrastraría un `COLLATE \"default\"` que nadie escribió.
               LEFT JOIN pg_catalog.pg_type t ON t.oid = a.atttypid
               LEFT JOIN pg_catalog.pg_collation co
                      ON co.oid = a.attcollation AND a.attcollation <> t.typcollation
              WHERE n.nspname = $1
                AND c.relkind IN ('r', 'p', 'f')
                AND NOT c.relispartition
                AND a.attnum > 0
                AND NOT a.attisdropped
              ORDER BY a.attrelid, a.attnum",
            &[&schema],
        )
        .await?;

    let mut map: HashMap<u32, Vec<Column>> = HashMap::new();
    for row in &rows {
        let expression: Option<String> = row.get(4);
        let generated: String = row.get(6);
        let identity: String = row.get(5);
        // Una columna generada guarda su expresión en `pg_attrdef`, igual que un `DEFAULT`, pero no
        // es uno: se separan acá para no proponer después un `SET DEFAULT` imposible.
        let (default, generated) = if generated.is_empty() {
            (expression, None)
        } else {
            (None, expression)
        };

        map.entry(row.get(0)).or_default().push(Column {
            name: row.get(1),
            type_name: row.get(2),
            not_null: row.get(3),
            default,
            identity: match identity.as_str() {
                "a" => Some(Identity::Always),
                "d" => Some(Identity::ByDefault),
                _ => None,
            },
            generated,
            collation: row.get(7),
        });
    }
    Ok(map)
}

async fn constraints(
    client: &tokio_postgres::Client,
    schema: &str,
) -> Result<HashMap<u32, Vec<NamedDef>>> {
    let rows = client
        .query(
            "SELECT con.conrelid,
                    con.conname::text,
                    pg_catalog.pg_get_constraintdef(con.oid, true)
               FROM pg_catalog.pg_constraint con
               JOIN pg_catalog.pg_class c ON c.oid = con.conrelid
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1
                AND NOT c.relispartition
                AND con.contype IN ('p', 'u', 'f', 'c', 'x')
              ORDER BY con.conrelid, con.conname",
            &[&schema],
        )
        .await?;

    let mut map: HashMap<u32, Vec<NamedDef>> = HashMap::new();
    for row in &rows {
        map.entry(row.get(0)).or_default().push(NamedDef {
            name: row.get(1),
            definition: row.get(2),
        });
    }
    Ok(map)
}

async fn indexes(
    client: &tokio_postgres::Client,
    schema: &str,
) -> Result<HashMap<u32, Vec<NamedDef>>> {
    let rows = client
        .query(
            "SELECT i.indrelid,
                    ic.relname::text,
                    pg_catalog.pg_get_indexdef(i.indexrelid)
               FROM pg_catalog.pg_index i
               JOIN pg_catalog.pg_class ic ON ic.oid = i.indexrelid
               JOIN pg_catalog.pg_class c ON c.oid = i.indrelid
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1
                AND c.relkind IN ('r', 'p', 'm')
                AND NOT c.relispartition
                -- El índice que respalda una clave primaria o única ya viaja como restricción.
                AND NOT EXISTS (SELECT 1
                                  FROM pg_catalog.pg_constraint con
                                 WHERE con.conindid = i.indexrelid
                                   AND con.contype IN ('p', 'u', 'x'))
              ORDER BY i.indrelid, ic.relname",
            &[&schema],
        )
        .await?;

    let mut map: HashMap<u32, Vec<NamedDef>> = HashMap::new();
    for row in &rows {
        map.entry(row.get(0)).or_default().push(NamedDef {
            name: row.get(1),
            definition: row.get(2),
        });
    }
    Ok(map)
}

async fn sequences(client: &tokio_postgres::Client, schema: &str) -> Result<Vec<Sequence>> {
    let rows = client
        .query(
            "SELECT c.relname::text,
                    pg_catalog.format_type(s.seqtypid, NULL),
                    s.seqstart, s.seqincrement, s.seqmin, s.seqmax, s.seqcache, s.seqcycle
               FROM pg_catalog.pg_sequence s
               JOIN pg_catalog.pg_class c ON c.oid = s.seqrelid
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1
                AND NOT EXISTS (SELECT 1
                                  FROM pg_catalog.pg_depend d
                                 WHERE d.objid = s.seqrelid
                                   AND d.classid = 'pg_class'::regclass
                                   AND d.deptype IN ('a', 'i'))
              ORDER BY c.relname",
            &[&schema],
        )
        .await?;

    Ok(rows
        .iter()
        .map(|row| Sequence {
            name: row.get(0),
            type_name: row.get(1),
            start: row.get(2),
            increment: row.get(3),
            min_value: row.get(4),
            max_value: row.get(5),
            cache: row.get(6),
            cycle: row.get(7),
        })
        .collect())
}

async fn types(client: &tokio_postgres::Client, schema: &str) -> Result<Vec<TypeDef>> {
    let rows = client
        .query(
            "SELECT t.oid,
                    t.typname::text,
                    t.typtype::text,
                    CASE WHEN t.typtype = 'd'
                         THEN pg_catalog.format_type(t.typbasetype, t.typtypmod)
                         WHEN t.typtype = 'r'
                         THEN (SELECT pg_catalog.format_type(r.rngsubtype, NULL)
                                 FROM pg_catalog.pg_range r
                                WHERE r.rngtypid = t.oid)
                    END,
                    t.typnotnull,
                    t.typdefault
               FROM pg_catalog.pg_type t
               JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
              WHERE n.nspname = $1
                -- Los rangos múltiples ('m') se crean solos con su rango: compararlos sería contar
                -- dos veces el mismo tipo.
                AND t.typtype IN ('e', 'c', 'd', 'r')
                -- Toda tabla tiene su tipo compuesto asociado; solo interesan los tipos que alguien
                -- escribió con un CREATE TYPE.
                AND (t.typrelid = 0
                     OR (SELECT c.relkind
                           FROM pg_catalog.pg_class c
                          WHERE c.oid = t.typrelid) = 'c')
                -- El tipo arreglo que acompaña a cada tipo tampoco se declara a mano.
                AND NOT EXISTS (SELECT 1
                                  FROM pg_catalog.pg_type el
                                 WHERE el.oid = t.typelem AND el.typarray = t.oid)
              ORDER BY t.typname",
            &[&schema],
        )
        .await?;

    let mut labels = enum_labels(client, schema).await?;
    let mut fields = composite_fields(client, schema).await?;
    let mut checks = domain_checks(client, schema).await?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            let oid: u32 = row.get(0);
            let typtype: String = row.get(2);
            let kind = match typtype.as_str() {
                "e" => TypeKind::Enum,
                "c" => TypeKind::Composite,
                "d" => TypeKind::Domain,
                "r" => TypeKind::Range,
                _ => return None,
            };

            Some(TypeDef {
                name: row.get(1),
                kind,
                labels: labels.remove(&oid).unwrap_or_default(),
                fields: fields.remove(&oid).unwrap_or_default(),
                base: row.get(3),
                not_null: row.get(4),
                default: row.get(5),
                checks: checks.remove(&oid).unwrap_or_default(),
            })
        })
        .collect())
}

async fn enum_labels(
    client: &tokio_postgres::Client,
    schema: &str,
) -> Result<HashMap<u32, Vec<String>>> {
    let rows = client
        .query(
            "SELECT e.enumtypid, e.enumlabel::text
               FROM pg_catalog.pg_enum e
               JOIN pg_catalog.pg_type t ON t.oid = e.enumtypid
               JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
              WHERE n.nspname = $1
              ORDER BY e.enumtypid, e.enumsortorder",
            &[&schema],
        )
        .await?;

    let mut map: HashMap<u32, Vec<String>> = HashMap::new();
    for row in &rows {
        map.entry(row.get(0)).or_default().push(row.get(1));
    }
    Ok(map)
}

async fn composite_fields(
    client: &tokio_postgres::Client,
    schema: &str,
) -> Result<HashMap<u32, Vec<Field>>> {
    let rows = client
        .query(
            "SELECT t.oid,
                    a.attname::text,
                    pg_catalog.format_type(a.atttypid, a.atttypmod)
               FROM pg_catalog.pg_type t
               JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
               JOIN pg_catalog.pg_attribute a ON a.attrelid = t.typrelid
              WHERE n.nspname = $1
                AND t.typtype = 'c'
                AND a.attnum > 0
                AND NOT a.attisdropped
              ORDER BY t.oid, a.attnum",
            &[&schema],
        )
        .await?;

    let mut map: HashMap<u32, Vec<Field>> = HashMap::new();
    for row in &rows {
        map.entry(row.get(0)).or_default().push(Field {
            name: row.get(1),
            type_name: row.get(2),
        });
    }
    Ok(map)
}

async fn domain_checks(
    client: &tokio_postgres::Client,
    schema: &str,
) -> Result<HashMap<u32, Vec<NamedDef>>> {
    let rows = client
        .query(
            "SELECT con.contypid,
                    con.conname::text,
                    pg_catalog.pg_get_constraintdef(con.oid, true)
               FROM pg_catalog.pg_constraint con
               JOIN pg_catalog.pg_type t ON t.oid = con.contypid
               JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
              WHERE n.nspname = $1
              ORDER BY con.contypid, con.conname",
            &[&schema],
        )
        .await?;

    let mut map: HashMap<u32, Vec<NamedDef>> = HashMap::new();
    for row in &rows {
        map.entry(row.get(0)).or_default().push(NamedDef {
            name: row.get(1),
            definition: row.get(2),
        });
    }
    Ok(map)
}
