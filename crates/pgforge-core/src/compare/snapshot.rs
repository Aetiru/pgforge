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
//! - Las **funciones de una extensión** (`pg_depend` con `deptype = 'e'`) tampoco: las trae
//!   `CREATE EXTENSION`, y compararlas haría que cada versión de la extensión se viera como cambio.
//! - Los **disparadores internos** (`tgisinternal`) son los que PostgreSQL arma para las claves
//!   foráneas; el usuario no los escribió y se recrean con la restricción.
//!
//! Los permisos se leen con `acldefault` de respaldo: una ACL en `NULL` no significa «sin permisos»
//! sino «los de fábrica» (el dueño con todo, y `EXECUTE` para `PUBLIC` en las funciones), y sin
//! expandirla un `REVOKE` explícito de un lado se vería igual a no haber tocado nada del otro. El
//! dueño se descarta: sus privilegios son suyos por serlo, y el rol dueño puede llamarse distinto
//! en cada servidor.
//!
//! Tampoco se leen datos: `last_value` de una secuencia es estado, no estructura, y traerlo haría
//! que dos esquemas idénticos aparezcan distintos por el solo hecho de que uno se usó.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::conn::ServerHandle;
use crate::ddl::table::Identity;
// `TypeKind` es el mismo `ddl::types::ShapeKind` con otro nombre en este módulo: acá compara
// esquemas y no cruza el IPC, así que no hace falta el nombre que ve la interfaz.
pub use crate::ddl::types::Field;
pub use crate::ddl::types::ShapeKind as TypeKind;
use crate::error::{Error, Result};
use crate::ServerVersion;

/// Un esquema entero, tal como estaba cuando se leyó.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// Funciones y procedimientos. Las agregaciones quedan afuera: `pg_get_functiondef` no las
    /// escribe.
    pub functions: Vec<Function>,
    pub triggers: Vec<Trigger>,
    pub policies: Vec<Policy>,
    /// Solo las tablas con seguridad por filas encendida o forzada; que una no figure significa
    /// que tiene las dos apagadas.
    pub row_security: Vec<RowSecurity>,
    /// Los permisos de cada tabla, vista, secuencia y función. Figuran **todos**, también los que
    /// no tienen ningún permiso para otro rol: saber que el objeto existe es lo que permite decidir
    /// si sus permisos viajan con él o hay que compararlos.
    pub acls: Vec<ObjectAcl>,
}

/// Un objeto cuyo texto ya lo escribe el servidor: una restricción o un índice.
///
/// Se guarda la definición tal cual la devuelve `pg_get_constraintdef` / `pg_get_indexdef` en vez de
/// desarmarla en campos: es la que el propio PostgreSQL considera correcta, y volver a armarla a
/// mano solo agrega maneras de equivocarse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedDef {
    pub name: String,
    pub definition: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelationKind {
    Ordinary,
    Partitioned,
    Foreign,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub name: String,
    pub materialized: bool,
    pub definition: String,
    /// Una vista materializada sí puede tener índices propios.
    pub indexes: Vec<NamedDef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
        functions: functions(&client, schema).await?,
        triggers: triggers(&client, schema).await?,
        policies: policies(&client, schema).await?,
        row_security: row_security(&client, schema).await?,
        acls: acls(&client, schema).await?,
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
            data_type: row.get(2),
            // La comparación no distingue por intercalado: no se lee acá, mismo comportamiento
            // que antes de compartir `Field` con `ddl::types`.
            collation: None,
        });
    }
    Ok(map)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FunctionKind {
    /// Función normal, de ventana o de disparador: se escriben igual.
    Function,
    Procedure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Function {
    pub name: String,
    /// Los tipos de los argumentos de entrada (`oidvectortypes`), sin sus nombres. Junto con el
    /// nombre es la identidad de la función: PostgreSQL permite varias con el mismo nombre y
    /// argumentos distintos, y compararlas por nombre solo las mezclaría. Los nombres de los
    /// argumentos no cuentan para la identidad: `f(a int)` y `f(b int)` son la misma función.
    pub arguments: String,
    pub kind: FunctionKind,
    /// Tal cual la escribe `pg_get_functiondef`: un `CREATE OR REPLACE` completo.
    pub definition: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TriggerEnabled {
    /// Dispara con la replicación en modo normal (`origin`): el estado de fábrica.
    Origin,
    Disabled,
    Replica,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trigger {
    /// Tabla o vista de la que cuelga.
    pub table: String,
    pub name: String,
    /// De `pg_get_triggerdef`.
    pub definition: String,
    pub enabled: TriggerEnabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PolicyCommand {
    All,
    Select,
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub table: String,
    pub name: String,
    pub command: PolicyCommand,
    pub permissive: bool,
    /// Ordenados; `PUBLIC` si es para todos. Un rol que ya no existe no figura.
    pub roles: Vec<String>,
    /// Las dos expresiones salen de `pg_get_expr` y son `None` cuando la política no las tiene.
    pub using: Option<String>,
    pub with_check: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowSecurity {
    pub table: String,
    pub enabled: bool,
    /// `FORCE`: vale también para el dueño de la tabla.
    pub forced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AclKind {
    /// Tabla, vista o vista materializada: todas se otorgan con `ON TABLE`.
    Table,
    Sequence,
    Function,
    Procedure,
}

/// El permiso de un rol sobre un objeto, ya desarmado de la ACL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    /// Nombre del rol, o `PUBLIC`. Un rol no puede llamarse así, así que no hay ambigüedad.
    pub grantee: String,
    pub privilege: String,
    /// `WITH GRANT OPTION`: puede dárselo a otros.
    pub grantable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectAcl {
    pub kind: AclKind,
    pub name: String,
    /// Solo en funciones y procedimientos, con el mismo sentido que [`Function::arguments`].
    pub arguments: Option<String>,
    /// Sin los del dueño.
    pub grants: Vec<Grant>,
}

/// Los privilegios que puede traer una ACL. Lo usa la validación de una instantánea guardada, que
/// interpola el nombre del privilegio sin comillas.
pub const PRIVILEGES: &[&str] = &[
    "SELECT",
    "INSERT",
    "UPDATE",
    "DELETE",
    "TRUNCATE",
    "REFERENCES",
    "TRIGGER",
    "MAINTAIN",
    "USAGE",
    "EXECUTE",
];

async fn functions(client: &tokio_postgres::Client, schema: &str) -> Result<Vec<Function>> {
    let rows = client
        .query(
            "SELECT p.proname::text,
                    pg_catalog.oidvectortypes(p.proargtypes),
                    p.prokind::text,
                    pg_catalog.pg_get_functiondef(p.oid)
               FROM pg_catalog.pg_proc p
               JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
              WHERE n.nspname = $1
                -- Funciones, procedimientos y de ventana. Las agregaciones ('a') hacen fallar a
                -- `pg_get_functiondef`.
                AND p.prokind IN ('f', 'p', 'w')
                AND NOT EXISTS (SELECT 1
                                  FROM pg_catalog.pg_depend d
                                 WHERE d.classid = 'pg_catalog.pg_proc'::regclass
                                   AND d.objid = p.oid
                                   AND d.deptype = 'e')
              ORDER BY p.proname, p.oid",
            &[&schema],
        )
        .await?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            let kind: String = row.get(2);
            // Sin texto no hay nada que comparar ni que emitir: se omite en vez de inventar una
            // definición vacía que después pasaría por «distinta» de todas.
            let definition: Option<String> = row.get(3);
            Some(Function {
                name: row.get(0),
                arguments: row.get(1),
                kind: if kind == "p" {
                    FunctionKind::Procedure
                } else {
                    FunctionKind::Function
                },
                definition: definition?,
            })
        })
        .collect())
}

async fn triggers(client: &tokio_postgres::Client, schema: &str) -> Result<Vec<Trigger>> {
    let rows = client
        .query(
            "SELECT c.relname::text,
                    t.tgname::text,
                    pg_catalog.pg_get_triggerdef(t.oid, true),
                    t.tgenabled::text
               FROM pg_catalog.pg_trigger t
               JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1
                AND NOT t.tgisinternal
                -- Las particiones no se leen como tablas: sus disparadores son copias de los de la
                -- madre y aparecerían repetidos.
                AND NOT c.relispartition
                AND c.relkind IN ('r', 'p', 'f', 'v')
              ORDER BY c.relname, t.tgname",
            &[&schema],
        )
        .await?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            let enabled: String = row.get(3);
            let definition: Option<String> = row.get(2);
            Some(Trigger {
                table: row.get(0),
                name: row.get(1),
                definition: definition?,
                enabled: match enabled.as_str() {
                    "D" => TriggerEnabled::Disabled,
                    "R" => TriggerEnabled::Replica,
                    "A" => TriggerEnabled::Always,
                    _ => TriggerEnabled::Origin,
                },
            })
        })
        .collect())
}

async fn policies(client: &tokio_postgres::Client, schema: &str) -> Result<Vec<Policy>> {
    let rows = client
        .query(
            "SELECT c.relname::text,
                    pol.polname::text,
                    pol.polcmd::text,
                    pol.polpermissive,
                    -- 0 es PUBLIC; un rol borrado deja un NULL que se descarta abajo.
                    ARRAY(SELECT CASE WHEN r = 0 THEN 'PUBLIC'
                                      ELSE (SELECT ro.rolname::text
                                              FROM pg_catalog.pg_roles ro
                                             WHERE ro.oid = r) END
                            FROM pg_catalog.unnest(pol.polroles) AS r)::text[],
                    pg_catalog.pg_get_expr(pol.polqual, pol.polrelid),
                    pg_catalog.pg_get_expr(pol.polwithcheck, pol.polrelid)
               FROM pg_catalog.pg_policy pol
               JOIN pg_catalog.pg_class c ON c.oid = pol.polrelid
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1
                AND NOT c.relispartition
              ORDER BY c.relname, pol.polname",
            &[&schema],
        )
        .await?;

    Ok(rows
        .iter()
        .map(|row| {
            let command: String = row.get(2);
            let roles: Vec<Option<String>> = row.get(4);
            let mut roles: Vec<String> = roles.into_iter().flatten().collect();
            roles.sort();
            Policy {
                table: row.get(0),
                name: row.get(1),
                command: match command.as_str() {
                    "r" => PolicyCommand::Select,
                    "a" => PolicyCommand::Insert,
                    "w" => PolicyCommand::Update,
                    "d" => PolicyCommand::Delete,
                    _ => PolicyCommand::All,
                },
                permissive: row.get(3),
                roles,
                using: row.get(5),
                with_check: row.get(6),
            }
        })
        .collect())
}

async fn row_security(client: &tokio_postgres::Client, schema: &str) -> Result<Vec<RowSecurity>> {
    let rows = client
        .query(
            "SELECT c.relname::text, c.relrowsecurity, c.relforcerowsecurity
               FROM pg_catalog.pg_class c
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
              WHERE n.nspname = $1
                AND c.relkind IN ('r', 'p')
                AND NOT c.relispartition
                AND (c.relrowsecurity OR c.relforcerowsecurity)
              ORDER BY c.relname",
            &[&schema],
        )
        .await?;

    Ok(rows
        .iter()
        .map(|row| RowSecurity {
            table: row.get(0),
            enabled: row.get(1),
            forced: row.get(2),
        })
        .collect())
}

/// Un permiso suelto, si la fila de la consulta trae uno.
///
/// Las consultas de [`acls`] devuelven además una fila con todo en `NULL` para el objeto que no
/// tiene ningún permiso (`LEFT JOIN LATERAL`): sin ella el objeto no figuraría y no habría cómo
/// distinguir «existe sin permisos» de «no existe». Por eso las tres columnas se leen como `Option`.
fn grant_of(
    grantee: Option<String>,
    privilege: Option<String>,
    grantable: Option<bool>,
) -> Option<Grant> {
    Some(Grant {
        grantee: grantee?,
        privilege: privilege?,
        grantable: grantable.unwrap_or(false),
    })
}

/// Permisos de tablas, vistas, secuencias, funciones y procedimientos.
async fn acls(client: &tokio_postgres::Client, schema: &str) -> Result<Vec<ObjectAcl>> {
    let relations = client
        .query(
            "SELECT c.relname::text,
                    c.relkind::text,
                    CASE a.grantee WHEN 0 THEN 'PUBLIC' ELSE r.rolname::text END,
                    a.privilege_type::text,
                    a.is_grantable
               FROM pg_catalog.pg_class c
               JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
               LEFT JOIN LATERAL pg_catalog.aclexplode(
                         COALESCE(c.relacl,
                                  -- El CASE da `text` y `acldefault` pide `\"char\"`.
                                  pg_catalog.acldefault((CASE WHEN c.relkind = 'S'
                                                              THEN 's' ELSE 'r' END)::\"char\",
                                                        c.relowner))) AS a
                      ON a.grantee <> c.relowner
               LEFT JOIN pg_catalog.pg_roles r ON r.oid = a.grantee
              WHERE n.nspname = $1
                AND c.relkind IN ('r', 'p', 'f', 'v', 'm', 'S')
                AND NOT c.relispartition
              ORDER BY c.relname, 3, 4",
            &[&schema],
        )
        .await?;

    let routines = client
        .query(
            "SELECT p.proname::text,
                    p.prokind::text,
                    pg_catalog.oidvectortypes(p.proargtypes),
                    CASE a.grantee WHEN 0 THEN 'PUBLIC' ELSE r.rolname::text END,
                    a.privilege_type::text,
                    a.is_grantable
               FROM pg_catalog.pg_proc p
               JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
               LEFT JOIN LATERAL pg_catalog.aclexplode(
                         COALESCE(p.proacl, pg_catalog.acldefault('f', p.proowner))) AS a
                      ON a.grantee <> p.proowner
               LEFT JOIN pg_catalog.pg_roles r ON r.oid = a.grantee
              WHERE n.nspname = $1
                AND p.prokind IN ('f', 'p', 'w')
                AND NOT EXISTS (SELECT 1
                                  FROM pg_catalog.pg_depend d
                                 WHERE d.classid = 'pg_catalog.pg_proc'::regclass
                                   AND d.objid = p.oid
                                   AND d.deptype = 'e')
              ORDER BY p.proname, p.oid, 4, 5",
            &[&schema],
        )
        .await?;

    let mut out: Vec<ObjectAcl> = Vec::new();

    // Las filas de un mismo objeto vienen seguidas (por el ORDER BY): se abre uno nuevo cuando
    // cambia la clave y se le suman los permisos mientras no cambie. Dos funciones con el mismo
    // nombre y argumentos distintos tienen claves distintas.
    let mut push =
        |kind: AclKind, name: String, arguments: Option<String>, grant: Option<Grant>| {
            let same = out.last().is_some_and(|last| {
                last.kind == kind && last.name == name && last.arguments == arguments
            });
            if !same {
                out.push(ObjectAcl {
                    kind,
                    name,
                    arguments,
                    grants: Vec::new(),
                });
            }
            if let (Some(grant), Some(last)) = (grant, out.last_mut()) {
                last.grants.push(grant);
            }
        };

    for row in &relations {
        let relkind: String = row.get(1);
        push(
            if relkind == "S" {
                AclKind::Sequence
            } else {
                AclKind::Table
            },
            row.get(0),
            None,
            grant_of(row.get(2), row.get(3), row.get(4)),
        );
    }
    for row in &routines {
        let prokind: String = row.get(1);
        push(
            if prokind == "p" {
                AclKind::Procedure
            } else {
                AclKind::Function
            },
            row.get(0),
            Some(row.get(2)),
            grant_of(row.get(3), row.get(4), row.get(5)),
        );
    }

    Ok(out)
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
