//! La comparación en sí: dos instantáneas entran, una lista de diferencias sale.
//!
//! Es una función pura, y ahí está toda la gracia: lo que se puede equivocar —dar por distinta una
//! tabla que es igual, o al revés— se prueba sin levantar ningún PostgreSQL.
//!
//! Dos normalizaciones evitan diferencias que no lo son:
//!
//! - **El nombre del esquema.** `pg_get_indexdef` devuelve el índice con su tabla calificada, así
//!   que comparar `dev.pedidos` contra `prod.pedidos` marcaría cada índice como distinto. Antes de
//!   comparar, las referencias al esquema origen se reescriben con el nombre del destino
//!   ([`retarget`]), que es también lo que hace falta para poder ejecutar el SQL del otro lado.
//! - **Los espacios.** Dos servidores de versiones distintas escriben el mismo `SELECT` con saltos
//!   de línea en otro lugar. Se comparan sin espacios de más; lo que se muestra es siempre el texto
//!   tal cual vino.
//!
//! Lo igual no se reporta: un esquema con cuatrocientas tablas y dos diferencias tiene que
//! mostrarse como dos filas. Lo único que queda de lo demás es el conteo.

use serde::Serialize;

use crate::ddl::quote_ident;

use super::snapshot::{
    Column, NamedDef, RelationKind, SchemaSnapshot, Sequence, Table, TypeDef, TypeKind, View,
};
use super::{pair, render};

/// De dónde salió cada lado, para que la interfaz pueda decirlo sin volver a preguntar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SideInfo {
    /// Nombre del perfil de conexión.
    pub server: String,
    pub database: String,
    pub schema: String,
    pub version: String,
}

/// Qué es el objeto que difiere. Es el vocabulario con el que la interfaz elige ícono y color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectKind {
    Table,
    PartitionedTable,
    ForeignTable,
    View,
    MaterializedView,
    Sequence,
    Enum,
    Composite,
    Domain,
    Range,
}

/// De qué lado está lo que se encontró.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// Existe en el origen y falta en el destino.
    OnlySource,
    /// Existe en el destino y falta en el origen.
    OnlyTarget,
    /// Está en los dos, con alguna diferencia.
    Different,
}

/// Qué parte de un objeto difiere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DetailKind {
    Column,
    Constraint,
    Index,
    /// Un valor de una enumeración o un campo de un tipo compuesto.
    Member,
    /// Cualquier otra cosa del objeto mismo: el cuerpo de una vista, el incremento de una
    /// secuencia, la clave de particionado de una tabla.
    Property,
}

/// Una diferencia dentro de un objeto que existe de los dos lados.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub kind: DetailKind,
    pub name: String,
    pub status: Status,
    /// El texto del lado origen, tal como vino. Ausente cuando de ese lado no existe.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// Un objeto con algo distinto entre los dos lados.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntry {
    pub kind: ObjectKind,
    pub name: String,
    pub status: Status,
    /// El objeto entero de cada lado, para mostrarlos enfrentados. Es el mismo generador que usa
    /// el SQL de sincronización, así que lo que se lee es lo que se ejecutaría.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ddl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_ddl: Option<String>,
    pub details: Vec<Detail>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiff {
    pub source: SideInfo,
    pub target: SideInfo,
    pub entries: Vec<DiffEntry>,
    /// Cuántos objetos resultaron idénticos. No se listan, pero sin este número no se distingue
    /// «no hay diferencias» de «no se leyó nada».
    pub equal: usize,
}

impl SchemaDiff {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Reescribe las referencias al esquema `from` como si fueran a `to`.
///
/// Hace falta en los dos sentidos: para comparar `dev.pedidos` con `prod.pedidos` sin que el nombre
/// del esquema cuente como diferencia, y para que el SQL generado a partir del origen apunte al
/// destino. Con el mismo nombre de los dos lados —el caso normal— no toca nada.
///
/// Es un reemplazo de texto sobre el prefijo calificado, no un análisis del SQL: un alias que se
/// llame igual que el esquema también se reescribiría. Escribir un analizador de SQL para eso
/// costaría más de lo que arregla, y el resultado se muestra antes de ejecutarse.
pub fn retarget(sql: &str, from: &str, to: &str) -> String {
    if from == to {
        return sql.to_owned();
    }

    let quoted_from = quote_ident(from);
    let quoted_to = quote_ident(to);

    let mut out = sql.replace(&format!("{quoted_from}."), &format!("{quoted_to}."));
    // Un esquema con mayúsculas viaja citado en la definición, pero el nombre a secas puede
    // aparecer igual —por ejemplo en un `search_path` escrito a mano—, así que se cubren los dos.
    if quoted_from != from {
        out = out.replace(&format!("{from}."), &format!("{quoted_to}."));
    }
    out
}

/// Compara sin tener en cuenta espacios de más ni el nombre del esquema.
fn same_sql(source: &str, target: &str, from: &str, to: &str) -> bool {
    collapse(&retarget(source, from, to)) == collapse(target)
}

/// Deja una sola separación entre palabras. Solo para comparar: lo que se muestra nunca pasa por
/// acá.
fn collapse(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Compara el cuerpo de una vista, que es lo único que cambia de forma según la versión.
///
/// PostgreSQL 16 dejó de calificar cada columna con el nombre de su tabla en la salida de
/// `pg_get_viewdef`: la misma vista sale como `SELECT clientes.id … WHERE clientes.estado = …` en
/// PG 13 y como `SELECT id … WHERE estado = …` en PG 17. Comparando texto contra texto, cada vista
/// del esquema aparecería distinta al comparar dos versiones mayores, que es justamente lo que uno
/// hace antes de una actualización.
///
/// Por eso la segunda pasada —comparar sin esas calificaciones— **solo** se intenta cuando los dos
/// servidores son de versiones mayores distintas. Entre dos servidores de la misma versión la
/// comparación queda exacta, porque ahí una calificación de más sí es una diferencia escrita por
/// alguien.
pub(crate) fn same_view_body(
    source: &SchemaSnapshot,
    target: &SchemaSnapshot,
    left: &str,
    right: &str,
) -> bool {
    if same_sql(left, right, &source.schema, &target.schema) {
        return true;
    }
    if source.version.major() == target.version.major() {
        return false;
    }

    // Los nombres de esquema de las dos bases son lo único que no se saca: sin ellos, dos vistas
    // que leen la misma tabla de esquemas distintos pasarían por iguales.
    let mut keep: Vec<&str> = source.schemas.iter().map(String::as_str).collect();
    keep.extend(target.schemas.iter().map(String::as_str));

    same_sql(
        &unqualify(left, &keep),
        &unqualify(right, &keep),
        &source.schema,
        &target.schema,
    )
}

/// Saca los prefijos `algo.` salvo los de los esquemas, que son los que sí importan.
///
/// Es un barrido de texto y no un análisis de SQL, con el mismo criterio que [`retarget`]: alcanza
/// para lo que se necesita —tolerar el cambio de formato de PG 16— y lo que produce nunca se
/// muestra ni se ejecuta, solo se compara.
fn unqualify(sql: &str, keep: &[&str]) -> String {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut position = 0;

    while position < chars.len() {
        let start = position;
        // Un identificador empieza con letra o guion bajo; así un `1.5` no cuenta como prefijo.
        if chars[position].is_ascii_alphabetic() || chars[position] == '_' {
            while position < chars.len()
                && (chars[position].is_ascii_alphanumeric() || chars[position] == '_')
            {
                position += 1;
            }
            let word: String = chars[start..position].iter().collect();
            let qualifies = chars.get(position) == Some(&'.') && !keep.contains(&word.as_str());
            if qualifies {
                // Se descartan la palabra y el punto: lo que sigue queda sin calificar.
                position += 1;
                continue;
            }
            out.push_str(&word);
        } else {
            out.push(chars[position]);
            position += 1;
        }
    }

    out
}

fn side(server: &str, snapshot: &SchemaSnapshot) -> SideInfo {
    SideInfo {
        server: server.to_owned(),
        database: snapshot.database.clone(),
        schema: snapshot.schema.clone(),
        version: snapshot.version.to_string(),
    }
}

/// Compara dos esquemas. El origen es el estado deseado; el destino, el que habría que llevar hasta
/// ahí.
pub fn diff(
    source: &SchemaSnapshot,
    source_server: &str,
    target: &SchemaSnapshot,
    target_server: &str,
) -> SchemaDiff {
    let mut entries = Vec::new();
    let mut equal = 0;

    // El orden de la lista es el de las dependencias, no el alfabético: un tipo se crea antes que
    // la tabla que lo usa, y la tabla antes que la vista que la lee. Así el informe se lee en el
    // mismo orden en que se aplicaría.
    for (name, left, right) in pair(&source.types, &target.types, |t| t.name.as_str()) {
        push(
            &mut entries,
            &mut equal,
            type_entry(&name, left, right, source, target),
        );
    }
    for (name, left, right) in pair(&source.sequences, &target.sequences, |s| s.name.as_str()) {
        push(
            &mut entries,
            &mut equal,
            sequence_entry(&name, left, right, source, target),
        );
    }
    for (name, left, right) in pair(&source.tables, &target.tables, |t| t.name.as_str()) {
        push(
            &mut entries,
            &mut equal,
            table_entry(&name, left, right, source, target),
        );
    }
    for (name, left, right) in pair(&source.views, &target.views, |v| v.name.as_str()) {
        push(
            &mut entries,
            &mut equal,
            view_entry(&name, left, right, source, target),
        );
    }

    SchemaDiff {
        source: side(source_server, source),
        target: side(target_server, target),
        entries,
        equal,
    }
}

/// Agrega la entrada si hay algo que contar; si no, suma uno a los iguales.
fn push(entries: &mut Vec<DiffEntry>, equal: &mut usize, entry: Option<DiffEntry>) {
    match entry {
        Some(entry) => entries.push(entry),
        None => *equal += 1,
    }
}

fn table_kind(table: &Table) -> ObjectKind {
    match table.kind {
        RelationKind::Ordinary => ObjectKind::Table,
        RelationKind::Partitioned => ObjectKind::PartitionedTable,
        RelationKind::Foreign => ObjectKind::ForeignTable,
    }
}

/// Cómo se nombra cada clase de relación en el informe. El `Debug` del enum diría `Ordinary`, que
/// es el identificador de Rust y no algo que se le pueda mostrar a nadie.
fn relation_label(kind: RelationKind) -> &'static str {
    match kind {
        RelationKind::Ordinary => "tabla",
        RelationKind::Partitioned => "tabla particionada",
        RelationKind::Foreign => "tabla foránea",
    }
}

fn type_label(kind: TypeKind) -> &'static str {
    match kind {
        TypeKind::Enum => "enumeración",
        TypeKind::Composite => "tipo compuesto",
        TypeKind::Domain => "dominio",
        TypeKind::Range => "rango",
    }
}

fn table_entry(
    name: &str,
    source_table: Option<&Table>,
    target_table: Option<&Table>,
    source: &SchemaSnapshot,
    target: &SchemaSnapshot,
) -> Option<DiffEntry> {
    match (source_table, target_table) {
        (Some(table), None) => Some(DiffEntry {
            kind: table_kind(table),
            name: name.to_owned(),
            status: Status::OnlySource,
            source_ddl: Some(render::describe_table(&source.schema, table)),
            target_ddl: None,
            details: Vec::new(),
        }),
        (None, Some(table)) => Some(DiffEntry {
            kind: table_kind(table),
            name: name.to_owned(),
            status: Status::OnlyTarget,
            source_ddl: None,
            target_ddl: Some(render::describe_table(&target.schema, table)),
            details: Vec::new(),
        }),
        (Some(left), Some(right)) => {
            let mut details = Vec::new();

            if left.kind != right.kind {
                details.push(Detail {
                    kind: DetailKind::Property,
                    name: "tipo de relación".to_owned(),
                    status: Status::Different,
                    source: Some(relation_label(left.kind).to_owned()),
                    target: Some(relation_label(right.kind).to_owned()),
                });
            }
            if left.partition_by != right.partition_by {
                details.push(Detail {
                    kind: DetailKind::Property,
                    name: "particionado".to_owned(),
                    status: Status::Different,
                    source: left.partition_by.clone(),
                    target: right.partition_by.clone(),
                });
            }

            for (column, left, right) in pair(&left.columns, &right.columns, |c| c.name.as_str()) {
                if let Some(detail) = column_detail(&column, left, right, source, target) {
                    details.push(detail);
                }
            }
            defs_detail(
                &mut details,
                DetailKind::Constraint,
                &left.constraints,
                &right.constraints,
                source,
                target,
            );
            defs_detail(
                &mut details,
                DetailKind::Index,
                &left.indexes,
                &right.indexes,
                source,
                target,
            );

            (!details.is_empty()).then(|| DiffEntry {
                kind: table_kind(left),
                name: name.to_owned(),
                status: Status::Different,
                source_ddl: Some(render::describe_table(&source.schema, left)),
                target_ddl: Some(render::describe_table(&target.schema, right)),
                details,
            })
        }
        (None, None) => None,
    }
}

fn column_detail(
    name: &str,
    source_column: Option<&Column>,
    target_column: Option<&Column>,
    source: &SchemaSnapshot,
    target: &SchemaSnapshot,
) -> Option<Detail> {
    match (source_column, target_column) {
        (Some(column), None) => Some(Detail {
            kind: DetailKind::Column,
            name: name.to_owned(),
            status: Status::OnlySource,
            source: Some(render::column_clause(column)),
            target: None,
        }),
        (None, Some(column)) => Some(Detail {
            kind: DetailKind::Column,
            name: name.to_owned(),
            status: Status::OnlyTarget,
            source: None,
            target: Some(render::column_clause(column)),
        }),
        (Some(left), Some(right)) => {
            let left_sql = render::column_clause(left);
            let right_sql = render::column_clause(right);
            // El `DEFAULT` de una columna puede nombrar una secuencia del esquema —`nextval(…)`—,
            // así que también pasa por la reescritura antes de compararse.
            (!same_sql(&left_sql, &right_sql, &source.schema, &target.schema)).then(|| Detail {
                kind: DetailKind::Column,
                name: name.to_owned(),
                status: Status::Different,
                source: Some(left_sql),
                target: Some(right_sql),
            })
        }
        (None, None) => None,
    }
}

/// Empareja restricciones o índices, que se comparan igual: por nombre y por el texto que devolvió
/// el servidor.
fn defs_detail(
    details: &mut Vec<Detail>,
    kind: DetailKind,
    source_defs: &[NamedDef],
    target_defs: &[NamedDef],
    source: &SchemaSnapshot,
    target: &SchemaSnapshot,
) {
    for (name, left, right) in pair(source_defs, target_defs, |d| d.name.as_str()) {
        match (left, right) {
            (Some(def), None) => details.push(Detail {
                kind,
                name,
                status: Status::OnlySource,
                source: Some(def.definition.clone()),
                target: None,
            }),
            (None, Some(def)) => details.push(Detail {
                kind,
                name,
                status: Status::OnlyTarget,
                source: None,
                target: Some(def.definition.clone()),
            }),
            (Some(left), Some(right)) => {
                if !same_sql(
                    &left.definition,
                    &right.definition,
                    &source.schema,
                    &target.schema,
                ) {
                    details.push(Detail {
                        kind,
                        name,
                        status: Status::Different,
                        source: Some(left.definition.clone()),
                        target: Some(right.definition.clone()),
                    });
                }
            }
            (None, None) => {}
        }
    }
}

fn view_kind(view: &View) -> ObjectKind {
    if view.materialized {
        ObjectKind::MaterializedView
    } else {
        ObjectKind::View
    }
}

fn view_entry(
    name: &str,
    source_view: Option<&View>,
    target_view: Option<&View>,
    source: &SchemaSnapshot,
    target: &SchemaSnapshot,
) -> Option<DiffEntry> {
    match (source_view, target_view) {
        (Some(view), None) => Some(DiffEntry {
            kind: view_kind(view),
            name: name.to_owned(),
            status: Status::OnlySource,
            source_ddl: Some(render::describe_view(&source.schema, view)),
            target_ddl: None,
            details: Vec::new(),
        }),
        (None, Some(view)) => Some(DiffEntry {
            kind: view_kind(view),
            name: name.to_owned(),
            status: Status::OnlyTarget,
            source_ddl: None,
            target_ddl: Some(render::describe_view(&target.schema, view)),
            details: Vec::new(),
        }),
        (Some(left), Some(right)) => {
            let mut details = Vec::new();

            if left.materialized != right.materialized {
                details.push(Detail {
                    kind: DetailKind::Property,
                    name: "materializada".to_owned(),
                    status: Status::Different,
                    source: Some(left.materialized.to_string()),
                    target: Some(right.materialized.to_string()),
                });
            }
            if !same_view_body(source, target, &left.definition, &right.definition) {
                details.push(Detail {
                    kind: DetailKind::Property,
                    name: "definición".to_owned(),
                    status: Status::Different,
                    source: Some(left.definition.trim().to_owned()),
                    target: Some(right.definition.trim().to_owned()),
                });
            }
            defs_detail(
                &mut details,
                DetailKind::Index,
                &left.indexes,
                &right.indexes,
                source,
                target,
            );

            (!details.is_empty()).then(|| DiffEntry {
                kind: view_kind(left),
                name: name.to_owned(),
                status: Status::Different,
                source_ddl: Some(render::describe_view(&source.schema, left)),
                target_ddl: Some(render::describe_view(&target.schema, right)),
                details,
            })
        }
        (None, None) => None,
    }
}

fn sequence_entry(
    name: &str,
    source_sequence: Option<&Sequence>,
    target_sequence: Option<&Sequence>,
    source: &SchemaSnapshot,
    target: &SchemaSnapshot,
) -> Option<DiffEntry> {
    match (source_sequence, target_sequence) {
        (Some(sequence), None) => Some(DiffEntry {
            kind: ObjectKind::Sequence,
            name: name.to_owned(),
            status: Status::OnlySource,
            source_ddl: Some(render::create_sequence(&source.schema, sequence)),
            target_ddl: None,
            details: Vec::new(),
        }),
        (None, Some(sequence)) => Some(DiffEntry {
            kind: ObjectKind::Sequence,
            name: name.to_owned(),
            status: Status::OnlyTarget,
            source_ddl: None,
            target_ddl: Some(render::create_sequence(&target.schema, sequence)),
            details: Vec::new(),
        }),
        (Some(left), Some(right)) => {
            let mut details = Vec::new();
            let mut property = |name: &str, left: String, right: String| {
                if left != right {
                    details.push(Detail {
                        kind: DetailKind::Property,
                        name: name.to_owned(),
                        status: Status::Different,
                        source: Some(left),
                        target: Some(right),
                    });
                }
            };

            property("tipo", left.type_name.clone(), right.type_name.clone());
            property("inicio", left.start.to_string(), right.start.to_string());
            property(
                "incremento",
                left.increment.to_string(),
                right.increment.to_string(),
            );
            property(
                "mínimo",
                left.min_value.to_string(),
                right.min_value.to_string(),
            );
            property(
                "máximo",
                left.max_value.to_string(),
                right.max_value.to_string(),
            );
            property("caché", left.cache.to_string(), right.cache.to_string());
            property("ciclo", left.cycle.to_string(), right.cycle.to_string());

            (!details.is_empty()).then(|| DiffEntry {
                kind: ObjectKind::Sequence,
                name: name.to_owned(),
                status: Status::Different,
                source_ddl: Some(render::create_sequence(&source.schema, left)),
                target_ddl: Some(render::create_sequence(&target.schema, right)),
                details,
            })
        }
        (None, None) => None,
    }
}

fn type_kind(definition: &TypeDef) -> ObjectKind {
    match definition.kind {
        TypeKind::Enum => ObjectKind::Enum,
        TypeKind::Composite => ObjectKind::Composite,
        TypeKind::Domain => ObjectKind::Domain,
        TypeKind::Range => ObjectKind::Range,
    }
}

fn type_entry(
    name: &str,
    source_type: Option<&TypeDef>,
    target_type: Option<&TypeDef>,
    source: &SchemaSnapshot,
    target: &SchemaSnapshot,
) -> Option<DiffEntry> {
    match (source_type, target_type) {
        (Some(definition), None) => Some(DiffEntry {
            kind: type_kind(definition),
            name: name.to_owned(),
            status: Status::OnlySource,
            source_ddl: Some(render::create_type(&source.schema, definition)),
            target_ddl: None,
            details: Vec::new(),
        }),
        (None, Some(definition)) => Some(DiffEntry {
            kind: type_kind(definition),
            name: name.to_owned(),
            status: Status::OnlyTarget,
            source_ddl: None,
            target_ddl: Some(render::create_type(&target.schema, definition)),
            details: Vec::new(),
        }),
        (Some(left), Some(right)) => {
            let mut details = Vec::new();

            if left.kind != right.kind {
                details.push(Detail {
                    kind: DetailKind::Property,
                    name: "categoría".to_owned(),
                    status: Status::Different,
                    source: Some(type_label(left.kind).to_owned()),
                    target: Some(type_label(right.kind).to_owned()),
                });
            } else {
                match left.kind {
                    TypeKind::Enum => labels_detail(&mut details, &left.labels, &right.labels),
                    TypeKind::Composite => {
                        for (field, source_field, target_field) in
                            pair(&left.fields, &right.fields, |f| f.name.as_str())
                        {
                            let text = |field: Option<&super::snapshot::Field>| {
                                field.map(|f| f.type_name.clone())
                            };
                            let (source_text, target_text) =
                                (text(source_field), text(target_field));
                            let status = match (&source_text, &target_text) {
                                (Some(_), None) => Status::OnlySource,
                                (None, Some(_)) => Status::OnlyTarget,
                                (Some(a), Some(b)) if a != b => Status::Different,
                                _ => continue,
                            };
                            details.push(Detail {
                                kind: DetailKind::Member,
                                name: field,
                                status,
                                source: source_text,
                                target: target_text,
                            });
                        }
                    }
                    TypeKind::Domain | TypeKind::Range => {
                        if left.base != right.base {
                            details.push(Detail {
                                kind: DetailKind::Property,
                                name: "tipo base".to_owned(),
                                status: Status::Different,
                                source: left.base.clone(),
                                target: right.base.clone(),
                            });
                        }
                        if left.not_null != right.not_null {
                            details.push(Detail {
                                kind: DetailKind::Property,
                                name: "NOT NULL".to_owned(),
                                status: Status::Different,
                                source: Some(left.not_null.to_string()),
                                target: Some(right.not_null.to_string()),
                            });
                        }
                        if left.default != right.default {
                            details.push(Detail {
                                kind: DetailKind::Property,
                                name: "DEFAULT".to_owned(),
                                status: Status::Different,
                                source: left.default.clone(),
                                target: right.default.clone(),
                            });
                        }
                        defs_detail(
                            &mut details,
                            DetailKind::Constraint,
                            &left.checks,
                            &right.checks,
                            source,
                            target,
                        );
                    }
                }
            }

            (!details.is_empty()).then(|| DiffEntry {
                kind: type_kind(left),
                name: name.to_owned(),
                status: Status::Different,
                source_ddl: Some(render::create_type(&source.schema, left)),
                target_ddl: Some(render::create_type(&target.schema, right)),
                details,
            })
        }
        (None, None) => None,
    }
}

/// Los valores de una enumeración se comparan como conjunto y además en su orden: agregar uno al
/// final se puede hacer con un `ALTER TYPE`, pero tenerlos en otro orden no.
fn labels_detail(details: &mut Vec<Detail>, source: &[String], target: &[String]) {
    for label in source {
        if !target.contains(label) {
            details.push(Detail {
                kind: DetailKind::Member,
                name: label.clone(),
                status: Status::OnlySource,
                source: Some(label.clone()),
                target: None,
            });
        }
    }
    for label in target {
        if !source.contains(label) {
            details.push(Detail {
                kind: DetailKind::Member,
                name: label.clone(),
                status: Status::OnlyTarget,
                source: None,
                target: Some(label.clone()),
            });
        }
    }

    let common_source: Vec<_> = source.iter().filter(|l| target.contains(l)).collect();
    let common_target: Vec<_> = target.iter().filter(|l| source.contains(l)).collect();
    if common_source != common_target {
        details.push(Detail {
            kind: DetailKind::Property,
            name: "orden de los valores".to_owned(),
            status: Status::Different,
            source: Some(source.join(", ")),
            target: Some(target.join(", ")),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::snapshot::{Field, RelationKind};
    use crate::ServerVersion;

    fn snapshot(schema: &str) -> SchemaSnapshot {
        SchemaSnapshot {
            database: "app".to_owned(),
            schema: schema.to_owned(),
            version: ServerVersion::from_num(160_004),
            schemas: vec![schema.to_owned()],
            tables: Vec::new(),
            views: Vec::new(),
            sequences: Vec::new(),
            types: Vec::new(),
        }
    }

    fn column(name: &str, type_name: &str) -> Column {
        Column {
            name: name.to_owned(),
            type_name: type_name.to_owned(),
            not_null: false,
            default: None,
            identity: None,
            generated: None,
            collation: None,
        }
    }

    fn table(name: &str, columns: Vec<Column>) -> Table {
        Table {
            name: name.to_owned(),
            kind: RelationKind::Ordinary,
            partition_by: None,
            columns,
            constraints: Vec::new(),
            indexes: Vec::new(),
        }
    }

    #[test]
    fn dos_esquemas_iguales_no_tienen_diferencias() {
        let mut source = snapshot("public");
        source
            .tables
            .push(table("clientes", vec![column("id", "bigint")]));
        let target = source.clone();

        let result = diff(&source, "dev", &target, "prod");
        assert!(result.is_empty());
        assert_eq!(result.equal, 1);
    }

    #[test]
    fn una_tabla_que_falta_en_el_destino_sale_como_solo_origen() {
        let mut source = snapshot("public");
        source
            .tables
            .push(table("clientes", vec![column("id", "bigint")]));
        let target = snapshot("public");

        let result = diff(&source, "dev", &target, "prod");
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].status, Status::OnlySource);
        assert_eq!(result.entries[0].kind, ObjectKind::Table);
        assert!(result.entries[0]
            .source_ddl
            .as_ref()
            .unwrap()
            .contains("CREATE TABLE"));
        assert!(result.entries[0].target_ddl.is_none());
    }

    #[test]
    fn una_columna_que_sobra_en_el_destino_sale_como_detalle() {
        let mut source = snapshot("public");
        source
            .tables
            .push(table("clientes", vec![column("id", "bigint")]));
        let mut target = snapshot("public");
        target.tables.push(table(
            "clientes",
            vec![column("id", "bigint"), column("borrado", "boolean")],
        ));

        let result = diff(&source, "dev", &target, "prod");
        let entry = &result.entries[0];
        assert_eq!(entry.status, Status::Different);
        assert_eq!(entry.details.len(), 1);
        assert_eq!(entry.details[0].name, "borrado");
        assert_eq!(entry.details[0].status, Status::OnlyTarget);
    }

    #[test]
    fn el_tipo_de_una_columna_cuenta_como_diferencia() {
        let mut source = snapshot("public");
        source
            .tables
            .push(table("clientes", vec![column("id", "bigint")]));
        let mut target = snapshot("public");
        target
            .tables
            .push(table("clientes", vec![column("id", "integer")]));

        let result = diff(&source, "dev", &target, "prod");
        assert_eq!(result.entries[0].details[0].status, Status::Different);
        assert_eq!(
            result.entries[0].details[0].source.as_deref(),
            Some("id bigint")
        );
        assert_eq!(
            result.entries[0].details[0].target.as_deref(),
            Some("id integer")
        );
    }

    #[test]
    fn el_nombre_del_esquema_no_es_una_diferencia() {
        let index = |schema: &str| NamedDef {
            name: "clientes_nombre_idx".to_owned(),
            definition: format!(
                "CREATE INDEX clientes_nombre_idx ON {schema}.clientes USING btree (nombre)"
            ),
        };

        let mut source = snapshot("dev");
        let mut source_table = table("clientes", vec![column("id", "bigint")]);
        source_table.indexes.push(index("dev"));
        source.tables.push(source_table);

        let mut target = snapshot("prod");
        let mut target_table = table("clientes", vec![column("id", "bigint")]);
        target_table.indexes.push(index("prod"));
        target.tables.push(target_table);

        assert!(diff(&source, "dev", &target, "prod").is_empty());
    }

    #[test]
    fn los_espacios_de_mas_no_son_una_diferencia() {
        let view = |definition: &str| View {
            name: "activos".to_owned(),
            materialized: false,
            definition: definition.to_owned(),
            indexes: Vec::new(),
        };

        let mut source = snapshot("public");
        source.views.push(view(" SELECT id\n   FROM clientes;"));
        let mut target = snapshot("public");
        target.views.push(view("SELECT id FROM clientes;"));

        assert!(diff(&source, "dev", &target, "prod").is_empty());
    }

    #[test]
    fn una_enumeracion_reporta_los_valores_que_faltan() {
        let enumeration = |labels: &[&str]| TypeDef {
            name: "estado".to_owned(),
            kind: TypeKind::Enum,
            labels: labels.iter().map(|l| (*l).to_owned()).collect(),
            fields: Vec::new(),
            base: None,
            not_null: false,
            default: None,
            checks: Vec::new(),
        };

        let mut source = snapshot("public");
        source.types.push(enumeration(&["activo", "baja", "pausa"]));
        let mut target = snapshot("public");
        target.types.push(enumeration(&["activo", "baja"]));

        let result = diff(&source, "dev", &target, "prod");
        assert_eq!(result.entries[0].kind, ObjectKind::Enum);
        assert_eq!(result.entries[0].details.len(), 1);
        assert_eq!(result.entries[0].details[0].name, "pausa");
        assert_eq!(result.entries[0].details[0].status, Status::OnlySource);
    }

    #[test]
    fn un_compuesto_compara_campo_por_campo() {
        let composite = |type_name: &str| TypeDef {
            name: "direccion".to_owned(),
            kind: TypeKind::Composite,
            labels: Vec::new(),
            fields: vec![Field {
                name: "numero".to_owned(),
                type_name: type_name.to_owned(),
            }],
            base: None,
            not_null: false,
            default: None,
            checks: Vec::new(),
        };

        let mut source = snapshot("public");
        source.types.push(composite("integer"));
        let mut target = snapshot("public");
        target.types.push(composite("text"));

        let result = diff(&source, "dev", &target, "prod");
        assert_eq!(result.entries[0].details[0].kind, DetailKind::Member);
        assert_eq!(result.entries[0].details[0].status, Status::Different);
    }

    #[test]
    fn una_secuencia_reporta_solo_la_propiedad_que_cambio() {
        let sequence = |increment: i64| Sequence {
            name: "clientes_id_seq".to_owned(),
            type_name: "bigint".to_owned(),
            start: 1,
            increment,
            min_value: 1,
            max_value: i64::MAX,
            cache: 1,
            cycle: false,
        };

        let mut source = snapshot("public");
        source.sequences.push(sequence(1));
        let mut target = snapshot("public");
        target.sequences.push(sequence(10));

        let result = diff(&source, "dev", &target, "prod");
        assert_eq!(result.entries[0].details.len(), 1);
        assert_eq!(result.entries[0].details[0].name, "incremento");
    }

    #[test]
    fn entre_versiones_distintas_la_calificacion_de_columnas_no_cuenta() {
        // La misma vista, escrita por PG 13 y por PG 17.
        let vieja = " SELECT clientes.id,\n    clientes.nombre\n   FROM app.clientes\n  \
                     WHERE clientes.estado = 'activo'::app.estado;";
        let nueva = " SELECT id,\n    nombre\n   FROM app.clientes\n  \
                     WHERE estado = 'activo'::app.estado;";

        let pg13 = SchemaSnapshot {
            version: ServerVersion::from_num(130_023),
            schemas: vec!["app".to_owned(), "otro".to_owned()],
            ..snapshot("app")
        };
        let pg17 = SchemaSnapshot {
            version: ServerVersion::from_num(170_010),
            schemas: vec!["app".to_owned(), "otro".to_owned()],
            ..snapshot("app")
        };

        assert!(same_view_body(&pg13, &pg17, vieja, nueva));
        // Entre servidores de la misma versión, esa diferencia la escribió alguien.
        assert!(!same_view_body(&pg13, &pg13.clone(), vieja, nueva));
        // Y el esquema se conserva: sin él, dos vistas que leen tablas de esquemas distintos
        // pasarían por iguales.
        assert!(!same_view_body(
            &pg13,
            &pg17,
            " SELECT id FROM app.clientes;",
            " SELECT id FROM otro.clientes;"
        ));
    }

    #[test]
    fn reescribe_el_esquema_citado_y_sin_citar() {
        assert_eq!(
            retarget("ON dev.clientes", "dev", "prod"),
            "ON prod.clientes"
        );
        assert_eq!(
            retarget("ON \"Dev\".clientes", "Dev", "prod"),
            "ON prod.clientes"
        );
        // Con el mismo nombre de los dos lados no toca nada.
        assert_eq!(retarget("ON dev.clientes", "dev", "dev"), "ON dev.clientes");
    }
}
