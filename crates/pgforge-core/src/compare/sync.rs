//! El SQL que llevaría el destino al estado del origen.
//!
//! Función pura, igual que [`super::diff`], y por el mismo motivo: acá es donde un `ALTER` mal
//! armado hace daño, así que tiene que poder probarse leyendo el texto que produce y sin ningún
//! servidor de por medio.
//!
//! **Nada de esto se ejecuta desde pgforge.** La comparación termina en un script que se muestra,
//! se copia o se abre en una pestaña de consulta; quien lo corre es el usuario, mirándolo, contra
//! el servidor que elija. Por eso cada sentencia viaja con su [`Risk`]: lo que borra datos se ve
//! distinto de lo que agrega una columna, y el orden en que salen es el orden en que se pueden
//! correr —tipos, secuencias, tablas, restricciones, índices, vistas, y al final lo destructivo—.
//!
//! Lo que PostgreSQL no puede hacer con un `ALTER` no se inventa: va a [`SyncPlan::warnings`] con
//! el motivo. Cambiar el tipo base de un dominio o reordenar los valores de una enumeración exige
//! recrear el objeto y todo lo que lo usa, y proponerlo en un script que alguien puede correr
//! entero sería la peor forma de enterarse.

use serde::Serialize;

use crate::ddl::quote_ident;
use crate::ddl::table::Identity;

use super::diff::{retarget, same_view_body, ObjectKind};
use super::pair;
use super::render::{self, qualified};
use super::snapshot::{
    Column, NamedDef, RelationKind, SchemaSnapshot, Sequence, Table, TypeDef, TypeKind, View,
};

/// Qué hace la sentencia con lo que ya existe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Risk {
    /// Agrega algo que no estaba. No puede perder nada.
    Safe,
    /// Puede fallar o tardar contra una tabla con datos: un `NOT NULL` sobre una columna con nulos,
    /// un cambio de tipo que reescribe la tabla entera.
    Review,
    /// Borra estructura, y con ella los datos que tenga adentro.
    Destructive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Create,
    Alter,
    Drop,
}

/// Una sentencia del script, con a qué objeto pertenece para poder filtrarla.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatement {
    pub object: ObjectKind,
    /// Nombre del objeto, el mismo con el que aparece en el informe.
    pub name: String,
    pub action: Action,
    pub risk: Risk,
    pub sql: String,
    /// Lo que hay que saber antes de correrla. Se escribe como comentario arriba de la sentencia.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlan {
    pub statements: Vec<SyncStatement>,
    /// Diferencias que no se pueden resolver con un `ALTER`, con el motivo.
    pub warnings: Vec<String>,
}

struct Builder<'a> {
    plan: SyncPlan,
    /// Esquema del origen: de ahí salen las definiciones que hay que reescribir.
    from: &'a str,
    /// Esquema del destino: contra ese se escribe todo el script.
    to: &'a str,
}

impl Builder<'_> {
    fn push(
        &mut self,
        object: ObjectKind,
        name: &str,
        action: Action,
        risk: Risk,
        sql: String,
        note: Option<&str>,
    ) {
        self.plan.statements.push(SyncStatement {
            object,
            name: name.to_owned(),
            action,
            risk,
            sql,
            note: note.map(str::to_owned),
        });
    }

    fn warn(&mut self, message: String) {
        self.plan.warnings.push(message);
    }

    /// Una definición del origen, escrita contra el esquema del destino.
    fn sql(&self, definition: &str) -> String {
        retarget(definition, self.from, self.to)
    }
}

/// Arma el script que llevaría `target` al estado de `source`.
pub fn plan(source: &SchemaSnapshot, target: &SchemaSnapshot) -> SyncPlan {
    let mut builder = Builder {
        plan: SyncPlan::default(),
        from: &source.schema,
        to: &target.schema,
    };

    types(&mut builder, source, target);
    sequences(&mut builder, source, target);
    tables(&mut builder, source, target);
    constraints_and_indexes(&mut builder, source, target);
    views(&mut builder, source, target);
    drops(&mut builder, source, target);

    builder.plan
}

/// El script entero, listo para pegar en el editor. Cada sentencia lleva su aviso arriba, como
/// comentario: si alguien copia el texto y se queda sin la lista, el aviso viaja con él.
pub fn script(statements: &[SyncStatement]) -> String {
    statements
        .iter()
        .map(|statement| match &statement.note {
            Some(note) => format!("-- {note}\n{}", statement.sql),
            None => statement.sql.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn type_kind(definition: &TypeDef) -> ObjectKind {
    match definition.kind {
        TypeKind::Enum => ObjectKind::Enum,
        TypeKind::Composite => ObjectKind::Composite,
        TypeKind::Domain => ObjectKind::Domain,
        TypeKind::Range => ObjectKind::Range,
    }
}

fn table_kind(table: &Table) -> ObjectKind {
    match table.kind {
        RelationKind::Ordinary => ObjectKind::Table,
        RelationKind::Partitioned => ObjectKind::PartitionedTable,
        RelationKind::Foreign => ObjectKind::ForeignTable,
    }
}

fn view_kind(view: &View) -> ObjectKind {
    if view.materialized {
        ObjectKind::MaterializedView
    } else {
        ObjectKind::View
    }
}

fn types(builder: &mut Builder, source: &SchemaSnapshot, target: &SchemaSnapshot) {
    for (name, left, right) in pair(&source.types, &target.types, |t| t.name.as_str()) {
        let (Some(left), right) = (left, right) else {
            continue;
        };

        let Some(right) = right else {
            let sql = builder.sql(&render::create_type(builder.from, left));
            builder.push(
                type_kind(left),
                &name,
                Action::Create,
                Risk::Safe,
                sql,
                None,
            );
            continue;
        };

        if left.kind != right.kind {
            builder.warn(format!(
                "el tipo «{name}» es de otra categoría en cada lado: hay que recrearlo a mano"
            ));
            continue;
        }

        match left.kind {
            TypeKind::Enum => enum_values(builder, &name, left, right),
            TypeKind::Composite => composite_attributes(builder, &name, left, right),
            TypeKind::Domain => domain(builder, &name, left, right),
            TypeKind::Range => {
                if left.base != right.base {
                    builder.warn(format!(
                        "el rango «{name}» tiene otro subtipo en cada lado: cambiarlo exige \
                         recrear el tipo y todo lo que lo usa"
                    ));
                }
            }
        }
    }
}

/// Los valores que le faltan a la enumeración del destino.
///
/// El `ALTER TYPE … ADD VALUE` va con `BEFORE` cuando el valor no queda último en el origen: sin
/// eso, agregar el que va al medio lo dejaría al final, y el orden de una enumeración es lo que
/// decide cómo ordena un `ORDER BY`.
fn enum_values(builder: &mut Builder, name: &str, source: &TypeDef, target: &TypeDef) {
    let qualified_name = qualified(builder.to, name);

    for (position, label) in source.labels.iter().enumerate() {
        if target.labels.contains(label) {
            continue;
        }

        // El primer valor posterior que el destino ya tiene: delante de ese va el nuevo.
        let next = source.labels[position + 1..]
            .iter()
            .find(|later| target.labels.contains(*later));

        let sql = match next {
            Some(next) => format!(
                "ALTER TYPE {qualified_name} ADD VALUE {} BEFORE {};",
                literal(label),
                literal(next)
            ),
            None => format!("ALTER TYPE {qualified_name} ADD VALUE {};", literal(label)),
        };
        builder.push(ObjectKind::Enum, name, Action::Alter, Risk::Safe, sql, None);
    }

    if target.labels.iter().any(|l| !source.labels.contains(l)) {
        builder.warn(format!(
            "la enumeración «{name}» tiene valores en el destino que no están en el origen: \
             PostgreSQL no puede quitar un valor de una enumeración"
        ));
    }
}

fn composite_attributes(builder: &mut Builder, name: &str, source: &TypeDef, target: &TypeDef) {
    let qualified_name = qualified(builder.to, name);

    for (field, left, right) in pair(&source.fields, &target.fields, |f| f.name.as_str()) {
        match (left, right) {
            (Some(left), None) => builder.push(
                ObjectKind::Composite,
                name,
                Action::Alter,
                Risk::Safe,
                format!(
                    "ALTER TYPE {qualified_name} ADD ATTRIBUTE {} {};",
                    quote_ident(&field),
                    builder.sql(&left.type_name)
                ),
                None,
            ),
            (None, Some(_)) => builder.push(
                ObjectKind::Composite,
                name,
                Action::Alter,
                Risk::Destructive,
                format!(
                    "ALTER TYPE {qualified_name} DROP ATTRIBUTE {};",
                    quote_ident(&field)
                ),
                Some("el campo desaparece del tipo y de todas las columnas que lo usan"),
            ),
            (Some(left), Some(right)) if left.type_name != right.type_name => builder.push(
                ObjectKind::Composite,
                name,
                Action::Alter,
                Risk::Review,
                format!(
                    "ALTER TYPE {qualified_name} ALTER ATTRIBUTE {} TYPE {};",
                    quote_ident(&field),
                    builder.sql(&left.type_name)
                ),
                Some("falla si algún valor guardado no se puede convertir al tipo nuevo"),
            ),
            _ => {}
        }
    }
}

fn domain(builder: &mut Builder, name: &str, source: &TypeDef, target: &TypeDef) {
    let qualified_name = qualified(builder.to, name);

    if source.base != target.base {
        builder.warn(format!(
            "el dominio «{name}» tiene otro tipo base en cada lado: PostgreSQL no permite \
             cambiarlo, hay que recrear el dominio"
        ));
    }

    if source.default != target.default {
        let sql = match &source.default {
            Some(default) => format!(
                "ALTER DOMAIN {qualified_name} SET DEFAULT {};",
                builder.sql(default)
            ),
            None => format!("ALTER DOMAIN {qualified_name} DROP DEFAULT;"),
        };
        builder.push(
            ObjectKind::Domain,
            name,
            Action::Alter,
            Risk::Safe,
            sql,
            None,
        );
    }

    if source.not_null != target.not_null {
        let (sql, risk, note) = if source.not_null {
            (
                format!("ALTER DOMAIN {qualified_name} SET NOT NULL;"),
                Risk::Review,
                Some("falla si alguna columna del dominio ya tiene nulos"),
            )
        } else {
            (
                format!("ALTER DOMAIN {qualified_name} DROP NOT NULL;"),
                Risk::Safe,
                None,
            )
        };
        builder.push(ObjectKind::Domain, name, Action::Alter, risk, sql, note);
    }

    for (check, left, right) in pair(&source.checks, &target.checks, |c| c.name.as_str()) {
        match (left, right) {
            (Some(left), None) => builder.push(
                ObjectKind::Domain,
                name,
                Action::Alter,
                Risk::Review,
                format!(
                    "ALTER DOMAIN {qualified_name} ADD CONSTRAINT {} {};",
                    quote_ident(&check),
                    builder.sql(&left.definition)
                ),
                Some("falla si algún valor guardado no cumple la condición"),
            ),
            (None, Some(_)) => builder.push(
                ObjectKind::Domain,
                name,
                Action::Alter,
                Risk::Destructive,
                format!(
                    "ALTER DOMAIN {qualified_name} DROP CONSTRAINT {};",
                    quote_ident(&check)
                ),
                None,
            ),
            (Some(left), Some(right)) if left.definition != right.definition => {
                builder.push(
                    ObjectKind::Domain,
                    name,
                    Action::Alter,
                    Risk::Destructive,
                    format!(
                        "ALTER DOMAIN {qualified_name} DROP CONSTRAINT {};",
                        quote_ident(&check)
                    ),
                    Some("la condición cambió: se borra y se vuelve a crear"),
                );
                builder.push(
                    ObjectKind::Domain,
                    name,
                    Action::Alter,
                    Risk::Review,
                    format!(
                        "ALTER DOMAIN {qualified_name} ADD CONSTRAINT {} {};",
                        quote_ident(&check),
                        builder.sql(&left.definition)
                    ),
                    None,
                );
            }
            _ => {}
        }
    }
}

fn sequences(builder: &mut Builder, source: &SchemaSnapshot, target: &SchemaSnapshot) {
    for (name, left, right) in pair(&source.sequences, &target.sequences, |s| s.name.as_str()) {
        let Some(left) = left else { continue };

        let Some(right) = right else {
            builder.push(
                ObjectKind::Sequence,
                &name,
                Action::Create,
                Risk::Safe,
                render::create_sequence(builder.to, left),
                None,
            );
            continue;
        };

        if let Some(sql) = alter_sequence(builder.to, left, right) {
            builder.push(
                ObjectKind::Sequence,
                &name,
                Action::Alter,
                Risk::Safe,
                sql,
                None,
            );
        }
    }
}

/// Solo las cláusulas que cambian: un `ALTER SEQUENCE` con todo repetido esconde lo que de verdad
/// se está tocando.
fn alter_sequence(schema: &str, source: &Sequence, target: &Sequence) -> Option<String> {
    let mut clauses = Vec::new();

    if source.type_name != target.type_name {
        clauses.push(format!("AS {}", source.type_name));
    }
    if source.start != target.start {
        clauses.push(format!("START WITH {}", source.start));
    }
    if source.increment != target.increment {
        clauses.push(format!("INCREMENT BY {}", source.increment));
    }
    if source.min_value != target.min_value {
        clauses.push(format!("MINVALUE {}", source.min_value));
    }
    if source.max_value != target.max_value {
        clauses.push(format!("MAXVALUE {}", source.max_value));
    }
    if source.cache != target.cache {
        clauses.push(format!("CACHE {}", source.cache));
    }
    if source.cycle != target.cycle {
        clauses.push(if source.cycle { "CYCLE" } else { "NO CYCLE" }.to_owned());
    }

    (!clauses.is_empty()).then(|| {
        format!(
            "ALTER SEQUENCE {} {};",
            qualified(schema, &source.name),
            clauses.join(" ")
        )
    })
}

/// Las tablas y sus columnas. Las restricciones y los índices van después, en su propio paso: una
/// clave foránea puede apuntar a una tabla que recién se crea unas líneas más abajo.
fn tables(builder: &mut Builder, source: &SchemaSnapshot, target: &SchemaSnapshot) {
    for (name, left, right) in pair(&source.tables, &target.tables, |t| t.name.as_str()) {
        let Some(left) = left else { continue };

        let Some(right) = right else {
            let sql = builder.sql(&render::create_table(builder.from, left));
            builder.push(
                table_kind(left),
                &name,
                Action::Create,
                Risk::Safe,
                sql,
                None,
            );
            continue;
        };

        if left.kind != right.kind || left.partition_by != right.partition_by {
            builder.warn(format!(
                "la tabla «{name}» está particionada distinto en cada lado: eso no se cambia con \
                 un ALTER, hay que recrearla"
            ));
        }

        let table = qualified(builder.to, &name);
        for (column, left_column, right_column) in
            pair(&left.columns, &right.columns, |c| c.name.as_str())
        {
            match (left_column, right_column) {
                (Some(left_column), None) => add_column(builder, left, &table, left_column),
                (None, Some(_)) => builder.push(
                    table_kind(left),
                    &name,
                    Action::Alter,
                    Risk::Destructive,
                    format!("ALTER TABLE {table} DROP COLUMN {};", quote_ident(&column)),
                    Some("la columna y sus datos desaparecen"),
                ),
                (Some(left_column), Some(right_column)) => {
                    alter_column(builder, left, &table, left_column, right_column)
                }
                (None, None) => {}
            }
        }
    }
}

fn add_column(builder: &mut Builder, table: &Table, qualified_table: &str, column: &Column) {
    let clause = builder.sql(&render::column_clause(column));
    let note = (column.not_null && column.default.is_none() && column.generated.is_none())
        .then_some("la columna es NOT NULL y no tiene DEFAULT: falla si la tabla tiene filas");

    builder.push(
        table_kind(table),
        &table.name,
        Action::Alter,
        if note.is_some() {
            Risk::Review
        } else {
            Risk::Safe
        },
        format!("ALTER TABLE {qualified_table} ADD COLUMN {clause};"),
        note,
    );
}

/// Una columna que existe de los dos lados, cosa por cosa.
///
/// Sale un `ALTER` por diferencia y no uno solo con todo adentro: el tipo, el `NOT NULL` y el
/// `DEFAULT` fallan por motivos distintos, y juntarlos haría que el que no se puede aplicar arrastre
/// a los que sí.
fn alter_column(
    builder: &mut Builder,
    table: &Table,
    qualified_table: &str,
    source: &Column,
    target: &Column,
) {
    let name = quote_ident(&source.name);
    // Se juntan primero y se agregan después: armarlas necesita leer el `Builder` —de ahí salen los
    // dos nombres de esquema— y agregarlas, escribirlo.
    let mut changes: Vec<(Risk, String, Option<&'static str>)> = Vec::new();
    let mut push =
        |risk: Risk, sql: String, note: Option<&'static str>| changes.push((risk, sql, note));

    // El tipo se compara ya reescrito: `format_type` califica los tipos del usuario con su esquema,
    // así que `dev.estado` y `prod.estado` son el mismo tipo escrito distinto.
    let type_name = builder.sql(&source.type_name);
    if type_name != target.type_name || source.collation != target.collation {
        let collate = source
            .collation
            .as_ref()
            .map(|c| format!(" COLLATE {}", quote_ident(c)))
            .unwrap_or_default();
        push(
            Risk::Review,
            format!("ALTER TABLE {qualified_table} ALTER COLUMN {name} TYPE {type_name}{collate};"),
            Some(
                "reescribe la tabla entera y puede necesitar un USING si la conversión no es \
                 automática",
            ),
        );
    }

    if source.default != target.default {
        match &source.default {
            Some(default) => push(
                Risk::Safe,
                format!(
                    "ALTER TABLE {qualified_table} ALTER COLUMN {name} SET DEFAULT {};",
                    retarget(default, builder.from, builder.to)
                ),
                None,
            ),
            None => push(
                Risk::Safe,
                format!("ALTER TABLE {qualified_table} ALTER COLUMN {name} DROP DEFAULT;"),
                None,
            ),
        }
    }

    if source.not_null != target.not_null {
        if source.not_null {
            push(
                Risk::Review,
                format!("ALTER TABLE {qualified_table} ALTER COLUMN {name} SET NOT NULL;"),
                Some("falla si la columna ya tiene nulos"),
            );
        } else {
            push(
                Risk::Safe,
                format!("ALTER TABLE {qualified_table} ALTER COLUMN {name} DROP NOT NULL;"),
                None,
            );
        }
    }

    if source.identity != target.identity {
        match (source.identity, target.identity) {
            (Some(identity), None) => push(
                Risk::Review,
                format!(
                    "ALTER TABLE {qualified_table} ALTER COLUMN {name} ADD GENERATED {} AS IDENTITY;",
                    identity_words(identity)
                ),
                Some("la columna tiene que estar sin DEFAULT y ser NOT NULL"),
            ),
            (None, Some(_)) => push(
                Risk::Destructive,
                format!("ALTER TABLE {qualified_table} ALTER COLUMN {name} DROP IDENTITY;"),
                Some("se borra la secuencia que generaba los valores"),
            ),
            (Some(identity), Some(_)) => push(
                Risk::Safe,
                format!(
                    "ALTER TABLE {qualified_table} ALTER COLUMN {name} SET GENERATED {};",
                    identity_words(identity)
                ),
                None,
            ),
            (None, None) => {}
        }
    }

    let kind = table_kind(table);
    for (risk, sql, note) in changes {
        builder.push(kind, &table.name, Action::Alter, risk, sql, note);
    }

    if source.generated != target.generated {
        builder.warn(format!(
            "la columna «{}.{}» es generada en un lado y no en el otro (o con otra expresión): \
             PostgreSQL no permite cambiarla, hay que borrarla y volver a crearla",
            table.name, source.name
        ));
    }
}

fn identity_words(identity: Identity) -> &'static str {
    match identity {
        Identity::Always => "ALWAYS",
        Identity::ByDefault => "BY DEFAULT",
    }
}

/// Restricciones e índices de las tablas que existen de los dos lados, más los de las que recién se
/// crean. Van juntos y al final de las tablas porque los dos pueden nombrar cualquier otra tabla
/// del esquema.
fn constraints_and_indexes(
    builder: &mut Builder,
    source: &SchemaSnapshot,
    target: &SchemaSnapshot,
) {
    for (name, left, right) in pair(&source.tables, &target.tables, |t| t.name.as_str()) {
        let Some(left) = left else { continue };
        let kind = table_kind(left);
        let table = qualified(builder.to, &name);
        let empty: Vec<NamedDef> = Vec::new();
        let (target_constraints, target_indexes) = match right {
            Some(right) => (&right.constraints, &right.indexes),
            None => (&empty, &empty),
        };

        for (constraint, left_def, right_def) in
            pair(&left.constraints, target_constraints, |c| c.name.as_str())
        {
            let add = |builder: &Builder, definition: &str| {
                format!(
                    "ALTER TABLE {table} ADD CONSTRAINT {} {};",
                    quote_ident(&constraint),
                    builder.sql(definition)
                )
            };

            match (left_def, right_def) {
                (Some(left_def), None) => {
                    let sql = add(builder, &left_def.definition);
                    builder.push(
                        kind,
                        &name,
                        Action::Alter,
                        Risk::Review,
                        sql,
                        Some("falla si los datos que ya están no cumplen la restricción"),
                    );
                }
                (Some(left_def), Some(right_def))
                    if left_def.definition != right_def.definition =>
                {
                    builder.push(
                        kind,
                        &name,
                        Action::Alter,
                        Risk::Destructive,
                        format!(
                            "ALTER TABLE {table} DROP CONSTRAINT {};",
                            quote_ident(&constraint)
                        ),
                        Some("la restricción cambió: se borra y se vuelve a crear"),
                    );
                    let sql = add(builder, &left_def.definition);
                    builder.push(kind, &name, Action::Alter, Risk::Review, sql, None);
                }
                _ => {}
            }
        }

        for (index, left_def, right_def) in pair(&left.indexes, target_indexes, |i| i.name.as_str())
        {
            match (left_def, right_def) {
                (Some(left_def), None) => {
                    let sql = format!("{};", builder.sql(&left_def.definition));
                    builder.push(kind, &name, Action::Create, Risk::Safe, sql, None);
                }
                (Some(left_def), Some(right_def))
                    if left_def.definition != right_def.definition =>
                {
                    builder.push(
                        kind,
                        &name,
                        Action::Drop,
                        Risk::Destructive,
                        format!("DROP INDEX {};", qualified(builder.to, &index)),
                        Some("el índice cambió: se borra y se vuelve a crear"),
                    );
                    let sql = format!("{};", builder.sql(&left_def.definition));
                    builder.push(kind, &name, Action::Create, Risk::Safe, sql, None);
                }
                _ => {}
            }
        }
    }
}

fn views(builder: &mut Builder, source: &SchemaSnapshot, target: &SchemaSnapshot) {
    for (name, left, right) in pair(&source.views, &target.views, |v| v.name.as_str()) {
        let Some(left) = left else { continue };
        let kind = view_kind(left);
        let create = |builder: &Builder, replace: bool| {
            builder.sql(&render::create_view(builder.from, left, replace))
        };

        match right {
            None => {
                let sql = create(builder, false);
                builder.push(kind, &name, Action::Create, Risk::Safe, sql, None);
            }
            Some(right) if left.materialized != right.materialized => builder.warn(format!(
                "«{name}» es una vista de un lado y una vista materializada del otro: hay que \
                 borrar la que está y crear la otra"
            )),
            Some(right) => {
                if !same_view_body(source, target, &left.definition, &right.definition) {
                    if left.materialized {
                        builder.push(
                            kind,
                            &name,
                            Action::Drop,
                            Risk::Destructive,
                            format!("DROP MATERIALIZED VIEW {};", qualified(builder.to, &name)),
                            Some(
                                "una vista materializada no se reemplaza: se borra, se crea de \
                                 nuevo y queda por refrescar",
                            ),
                        );
                        let sql = create(builder, false);
                        builder.push(kind, &name, Action::Create, Risk::Safe, sql, None);
                    } else {
                        let sql = create(builder, true);
                        builder.push(
                            kind,
                            &name,
                            Action::Alter,
                            Risk::Review,
                            sql,
                            Some(
                                "CREATE OR REPLACE falla si cambian los nombres o los tipos de \
                                 las columnas de la vista",
                            ),
                        );
                    }
                }

                for (index, left_def, right_def) in
                    pair(&left.indexes, &right.indexes, |i| i.name.as_str())
                {
                    match (left_def, right_def) {
                        (Some(left_def), None) => {
                            let sql = format!("{};", builder.sql(&left_def.definition));
                            builder.push(kind, &name, Action::Create, Risk::Safe, sql, None);
                        }
                        (Some(left_def), Some(right_def))
                            if left_def.definition != right_def.definition =>
                        {
                            builder.push(
                                kind,
                                &name,
                                Action::Drop,
                                Risk::Destructive,
                                format!("DROP INDEX {};", qualified(builder.to, &index)),
                                Some("el índice cambió: se borra y se vuelve a crear"),
                            );
                            let sql = format!("{};", builder.sql(&left_def.definition));
                            builder.push(kind, &name, Action::Create, Risk::Safe, sql, None);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Todo lo que sobra en el destino, al final y junto.
///
/// Va último por dos razones: es lo único que puede perder datos, y así el script se puede cortar
/// ahí y correr solo la mitad que agrega. El orden interno es el inverso al de creación —vistas,
/// tablas, secuencias, tipos—, porque lo que se borra primero es lo que depende de lo demás.
fn drops(builder: &mut Builder, source: &SchemaSnapshot, target: &SchemaSnapshot) {
    for view in &target.views {
        if !source.views.iter().any(|v| v.name == view.name) {
            let keyword = if view.materialized {
                "DROP MATERIALIZED VIEW"
            } else {
                "DROP VIEW"
            };
            builder.push(
                view_kind(view),
                &view.name,
                Action::Drop,
                Risk::Destructive,
                format!("{keyword} {};", qualified(builder.to, &view.name)),
                Some("no existe en el origen"),
            );
        }
    }

    for table in &target.tables {
        if !source.tables.iter().any(|t| t.name == table.name) {
            builder.push(
                table_kind(table),
                &table.name,
                Action::Drop,
                Risk::Destructive,
                format!("DROP TABLE {};", qualified(builder.to, &table.name)),
                Some("no existe en el origen: se pierden la tabla y sus datos"),
            );
        }
    }

    for sequence in &target.sequences {
        if !source.sequences.iter().any(|s| s.name == sequence.name) {
            builder.push(
                ObjectKind::Sequence,
                &sequence.name,
                Action::Drop,
                Risk::Destructive,
                format!("DROP SEQUENCE {};", qualified(builder.to, &sequence.name)),
                Some("no existe en el origen"),
            );
        }
    }

    for definition in &target.types {
        if !source.types.iter().any(|t| t.name == definition.name) {
            builder.push(
                type_kind(definition),
                &definition.name,
                Action::Drop,
                Risk::Destructive,
                format!("DROP TYPE {};", qualified(builder.to, &definition.name)),
                Some("no existe en el origen"),
            );
        }
    }
}

fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::snapshot::Field;
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

    fn enumeration(labels: &[&str]) -> TypeDef {
        TypeDef {
            name: "estado".to_owned(),
            kind: TypeKind::Enum,
            labels: labels.iter().map(|l| (*l).to_owned()).collect(),
            fields: Vec::new(),
            base: None,
            not_null: false,
            default: None,
            checks: Vec::new(),
        }
    }

    #[test]
    fn una_tabla_que_falta_se_crea_entera() {
        let mut source = snapshot("public");
        let mut nueva = table("clientes", vec![column("id", "bigint")]);
        nueva.constraints.push(NamedDef {
            name: "clientes_pkey".to_owned(),
            definition: "PRIMARY KEY (id)".to_owned(),
        });
        source.tables.push(nueva);

        let plan = plan(&source, &snapshot("public"));
        assert_eq!(plan.statements.len(), 2);
        assert_eq!(plan.statements[0].action, Action::Create);
        assert!(plan.statements[0]
            .sql
            .starts_with("CREATE TABLE public.clientes"));
        // La restricción va después de la tabla, no adentro: una foránea puede apuntar a una tabla
        // que se crea más abajo.
        assert!(plan.statements[1]
            .sql
            .contains("ADD CONSTRAINT clientes_pkey"));
    }

    #[test]
    fn el_script_apunta_al_esquema_del_destino() {
        let mut source = snapshot("dev");
        let mut tabla = table("clientes", vec![column("id", "bigint")]);
        tabla.indexes.push(NamedDef {
            name: "clientes_id_idx".to_owned(),
            definition: "CREATE INDEX clientes_id_idx ON dev.clientes USING btree (id)".to_owned(),
        });
        source.tables.push(tabla);

        let plan = plan(&source, &snapshot("prod"));
        assert!(plan.statements[0]
            .sql
            .starts_with("CREATE TABLE prod.clientes"));
        assert!(plan.statements[1].sql.contains("ON prod.clientes"));
    }

    #[test]
    fn una_columna_nueva_sin_default_avisa_que_puede_fallar() {
        let mut source = snapshot("public");
        source.tables.push(table(
            "clientes",
            vec![
                column("id", "bigint"),
                Column {
                    not_null: true,
                    ..column("email", "text")
                },
            ],
        ));
        let mut target = snapshot("public");
        target
            .tables
            .push(table("clientes", vec![column("id", "bigint")]));

        let plan = plan(&source, &target);
        assert_eq!(plan.statements.len(), 1);
        assert_eq!(plan.statements[0].risk, Risk::Review);
        assert_eq!(
            plan.statements[0].sql,
            "ALTER TABLE public.clientes ADD COLUMN email text NOT NULL;"
        );
        assert!(plan.statements[0].note.is_some());
    }

    #[test]
    fn una_columna_que_sobra_se_borra_al_final_y_marcada() {
        let mut source = snapshot("public");
        source
            .tables
            .push(table("clientes", vec![column("id", "bigint")]));
        let mut target = snapshot("public");
        target.tables.push(table(
            "clientes",
            vec![column("id", "bigint"), column("viejo", "text")],
        ));

        let plan = plan(&source, &target);
        assert_eq!(plan.statements[0].risk, Risk::Destructive);
        assert_eq!(
            plan.statements[0].sql,
            "ALTER TABLE public.clientes DROP COLUMN viejo;"
        );
    }

    #[test]
    fn cambiar_el_tipo_de_una_columna_queda_para_revisar() {
        let mut source = snapshot("public");
        source
            .tables
            .push(table("clientes", vec![column("id", "bigint")]));
        let mut target = snapshot("public");
        target
            .tables
            .push(table("clientes", vec![column("id", "integer")]));

        let plan = plan(&source, &target);
        assert_eq!(plan.statements.len(), 1);
        assert_eq!(plan.statements[0].risk, Risk::Review);
        assert_eq!(
            plan.statements[0].sql,
            "ALTER TABLE public.clientes ALTER COLUMN id TYPE bigint;"
        );
    }

    #[test]
    fn un_valor_nuevo_en_el_medio_de_una_enumeracion_va_con_before() {
        let mut source = snapshot("public");
        source.types.push(enumeration(&["activo", "pausa", "baja"]));
        let mut target = snapshot("public");
        target.types.push(enumeration(&["activo", "baja"]));

        let plan = plan(&source, &target);
        assert_eq!(
            plan.statements[0].sql,
            "ALTER TYPE public.estado ADD VALUE 'pausa' BEFORE 'baja';"
        );
    }

    #[test]
    fn un_valor_que_sobra_en_la_enumeracion_del_destino_solo_se_avisa() {
        let mut source = snapshot("public");
        source.types.push(enumeration(&["activo"]));
        let mut target = snapshot("public");
        target.types.push(enumeration(&["activo", "baja"]));

        let plan = plan(&source, &target);
        assert!(plan.statements.is_empty());
        assert_eq!(plan.warnings.len(), 1);
    }

    #[test]
    fn una_secuencia_solo_altera_lo_que_cambio() {
        let sequence = |increment: i64| Sequence {
            name: "cliente_id".to_owned(),
            type_name: "bigint".to_owned(),
            start: 1,
            increment,
            min_value: 1,
            max_value: i64::MAX,
            cache: 1,
            cycle: false,
        };

        let mut source = snapshot("public");
        source.sequences.push(sequence(2));
        let mut target = snapshot("public");
        target.sequences.push(sequence(1));

        let plan = plan(&source, &target);
        assert_eq!(
            plan.statements[0].sql,
            "ALTER SEQUENCE public.cliente_id INCREMENT BY 2;"
        );
    }

    #[test]
    fn una_vista_que_cambio_se_reemplaza() {
        let view = |definition: &str| View {
            name: "activos".to_owned(),
            materialized: false,
            definition: definition.to_owned(),
            indexes: Vec::new(),
        };

        let mut source = snapshot("public");
        source
            .views
            .push(view("SELECT id FROM clientes WHERE activo;"));
        let mut target = snapshot("public");
        target.views.push(view("SELECT id FROM clientes;"));

        let plan = plan(&source, &target);
        assert_eq!(plan.statements.len(), 1);
        assert!(plan.statements[0]
            .sql
            .starts_with("CREATE OR REPLACE VIEW public.activos AS"));
        assert_eq!(plan.statements[0].risk, Risk::Review);
    }

    #[test]
    fn lo_que_sobra_en_el_destino_se_borra_al_final() {
        let mut source = snapshot("public");
        source
            .tables
            .push(table("clientes", vec![column("id", "bigint")]));
        let mut target = snapshot("public");
        target
            .tables
            .push(table("clientes", vec![column("id", "bigint")]));
        target
            .tables
            .push(table("viejo", vec![column("id", "bigint")]));

        let plan = plan(&source, &target);
        assert_eq!(plan.statements.len(), 1);
        assert_eq!(plan.statements[0].action, Action::Drop);
        assert_eq!(plan.statements[0].risk, Risk::Destructive);
        assert_eq!(plan.statements[0].sql, "DROP TABLE public.viejo;");
    }

    #[test]
    fn un_campo_nuevo_de_un_compuesto_se_agrega() {
        let composite = |fields: Vec<Field>| TypeDef {
            name: "direccion".to_owned(),
            kind: TypeKind::Composite,
            labels: Vec::new(),
            fields,
            base: None,
            not_null: false,
            default: None,
            checks: Vec::new(),
        };

        let mut source = snapshot("public");
        source.types.push(composite(vec![
            Field {
                name: "calle".to_owned(),
                type_name: "text".to_owned(),
            },
            Field {
                name: "numero".to_owned(),
                type_name: "integer".to_owned(),
            },
        ]));
        let mut target = snapshot("public");
        target.types.push(composite(vec![Field {
            name: "calle".to_owned(),
            type_name: "text".to_owned(),
        }]));

        let plan = plan(&source, &target);
        assert_eq!(
            plan.statements[0].sql,
            "ALTER TYPE public.direccion ADD ATTRIBUTE numero integer;"
        );
    }

    #[test]
    fn el_script_escribe_cada_aviso_como_comentario() {
        let statements = vec![SyncStatement {
            object: ObjectKind::Table,
            name: "clientes".to_owned(),
            action: Action::Drop,
            risk: Risk::Destructive,
            sql: "DROP TABLE public.viejo;".to_owned(),
            note: Some("no existe en el origen".to_owned()),
        }];
        assert_eq!(
            script(&statements),
            "-- no existe en el origen\nDROP TABLE public.viejo;"
        );
    }
}
