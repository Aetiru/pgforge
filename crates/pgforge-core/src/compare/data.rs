//! Comparación de **datos** entre dos tablas: qué filas están solo en el origen, cuáles solo en el
//! destino y cuáles están en las dos pero difieren (y en qué columnas).
//!
//! Es la hermana de la comparación de esquemas y sigue su misma regla: lo único que toca el
//! servidor es la lectura. La comparación termina en un informe y en un script de `INSERT`,
//! `UPDATE` y `DELETE` que el usuario copia o abre en una pestaña de consulta *contra el destino*;
//! acá no hay un `apply`.
//!
//! Tres piezas, con el riesgo del lado de las puras: [`read_side`] abre un cursor por servidor,
//! [`merge`] los recorre a la vez (merge-join) y [`statements`] arma el script. Las dos últimas no
//! saben de servidores y se prueban con datos en memoria.
//!
//! **El orden lo decide el texto de la clave con colación `"C"`**, no el tipo. Dos servidores
//! pueden ordenar el mismo `text` distinto (colaciones, versiones de glibc), y un merge-join sobre
//! dos flujos ordenados con reglas distintas empareja mal sin avisar. Ordenar por `clave::text
//! COLLATE "C"` en los dos lados y comparar en Rust por bytes es la misma regla en ambos. El costo
//! es que el servidor no puede usar el índice de la clave para ordenar; a cambio, la memoria de
//! pgforge queda acotada a una fila por lado, sin importar el tamaño de la tabla.
//!
//! Todos los valores viajan como texto (`columna::text`), con `None` para NULL: así NULL y cadena
//! vacía no se confunden y no hace falta saber el tipo de cada columna. Entre versiones distintas
//! de PostgreSQL algún tipo puede escribirse distinto (un `float`, un `interval`) y aparecer como
//! diferencia sin serlo; es el precio de no interpretar los valores.

use std::cmp::Ordering;

use futures_util::stream::{Stream, StreamExt, TryStreamExt};
use serde::Serialize;

use crate::conn::{self, CancelSink, ServerHandle};
use crate::data::{self, TableShape};
use crate::ddl::quote_ident;
use crate::error::{Error, Result};

use super::sync::Risk;

/// Cuántas diferencias se reportan por omisión. Más allá de eso se sigue contando, pero ya no se
/// guardan: un millón de filas distintas no es un informe, y la interfaz no las dibujaría.
pub const DEFAULT_MAX_DIFFS: usize = 1000;

/// Cada cuántas filas leídas se avisa del avance.
const PROGRESS_EVERY: u64 = 2000;

/// Una fila leída: su clave (en texto, nunca nula) y los demás valores en el orden de `columns`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyedRow {
    pub key: Vec<String>,
    pub values: Vec<Option<String>>,
}

/// Avance de la lectura, para la barra de progreso.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub source_rows: u64,
    pub target_rows: u64,
}

/// Una diferencia entre las dos tablas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RowDiff {
    /// Está en el origen y falta en el destino.
    OnlySource {
        key: Vec<String>,
        values: Vec<Option<String>>,
    },
    /// Sobra en el destino.
    OnlyTarget {
        key: Vec<String>,
        values: Vec<Option<String>>,
    },
    /// Está en los dos con valores distintos. `changed` son posiciones dentro de `columns`.
    Different {
        key: Vec<String>,
        source: Vec<Option<String>>,
        target: Vec<Option<String>>,
        changed: Vec<usize>,
    },
}

/// Cuántas filas hay de cada clase, incluidas las que el tope dejó sin listar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub only_source: u64,
    pub only_target: u64,
    pub different: u64,
    pub equal: u64,
}

impl Counts {
    pub fn differences(&self) -> u64 {
        self.only_source + self.only_target + self.different
    }
}

/// El resultado crudo del merge-join.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Merged {
    pub rows: Vec<RowDiff>,
    pub counts: Counts,
    pub truncated: bool,
}

/// Recorre dos flujos ordenados por clave a la vez y devuelve lo que no coincide.
///
/// Memoria acotada: en cada paso hay a lo sumo una fila por lado en vuelo, más las diferencias
/// guardadas (hasta `max_diffs`). Verifica además que cada flujo venga estrictamente creciente:
/// si un servidor ordenara distinto de lo pedido, emparejar «a ciegas» daría un informe falso con
/// toda seguridad, así que se corta con un error en vez de seguir.
pub async fn merge<S, T>(
    mut source: S,
    mut target: T,
    max_diffs: usize,
    progress: &(dyn Fn(Progress) + Sync),
) -> Result<Merged>
where
    S: Stream<Item = Result<KeyedRow>> + Unpin,
    T: Stream<Item = Result<KeyedRow>> + Unpin,
{
    let mut out = Merged::default();
    let mut seen = Progress::default();
    let mut left = pull(&mut source, None, &mut seen.source_rows, "origen").await?;
    let mut right = pull(&mut target, None, &mut seen.target_rows, "destino").await?;
    let mut last_report = 0;

    let keep = |out: &mut Merged, diff: RowDiff| {
        if out.rows.len() < max_diffs {
            out.rows.push(diff);
        } else {
            out.truncated = true;
        }
    };

    loop {
        let order = match (&left, &right) {
            (None, None) => break,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(l), Some(r)) => l.key.cmp(&r.key),
        };

        match order {
            Ordering::Less => {
                let row = left.take().expect("la comparación vio una fila de origen");
                out.counts.only_source += 1;
                left = pull(&mut source, Some(&row.key), &mut seen.source_rows, "origen").await?;
                keep(
                    &mut out,
                    RowDiff::OnlySource {
                        key: row.key,
                        values: row.values,
                    },
                );
            }
            Ordering::Greater => {
                let row = right
                    .take()
                    .expect("la comparación vio una fila de destino");
                out.counts.only_target += 1;
                right = pull(
                    &mut target,
                    Some(&row.key),
                    &mut seen.target_rows,
                    "destino",
                )
                .await?;
                keep(
                    &mut out,
                    RowDiff::OnlyTarget {
                        key: row.key,
                        values: row.values,
                    },
                );
            }
            Ordering::Equal => {
                let l = left.take().expect("la comparación vio una fila de origen");
                let r = right
                    .take()
                    .expect("la comparación vio una fila de destino");
                left = pull(&mut source, Some(&l.key), &mut seen.source_rows, "origen").await?;
                right = pull(&mut target, Some(&r.key), &mut seen.target_rows, "destino").await?;
                if l.values == r.values {
                    out.counts.equal += 1;
                } else {
                    out.counts.different += 1;
                    let changed = l
                        .values
                        .iter()
                        .zip(&r.values)
                        .enumerate()
                        .filter(|(_, (a, b))| a != b)
                        .map(|(index, _)| index)
                        .collect();
                    keep(
                        &mut out,
                        RowDiff::Different {
                            key: l.key,
                            source: l.values,
                            target: r.values,
                            changed,
                        },
                    );
                }
            }
        }

        let total = seen.source_rows + seen.target_rows;
        if total - last_report >= PROGRESS_EVERY {
            last_report = total;
            progress(seen);
        }
    }

    progress(seen);
    Ok(out)
}

/// La fila siguiente de un flujo, verificando que la clave haya crecido respecto de la anterior.
async fn pull<S>(
    stream: &mut S,
    previous: Option<&Vec<String>>,
    counter: &mut u64,
    side: &str,
) -> Result<Option<KeyedRow>>
where
    S: Stream<Item = Result<KeyedRow>> + Unpin,
{
    let Some(row) = stream.next().await.transpose()? else {
        return Ok(None);
    };
    *counter += 1;
    if previous.is_some_and(|previous| row.key <= *previous) {
        return Err(Error::Config(format!(
            "el {side} devolvió las filas fuera de orden o con una clave repetida, y comparar así \
             emparejaría mal: la clave de la comparación tiene que ser única"
        )));
    }
    Ok(Some(row))
}

// ─── Lectura ────────────────────────────────────────────────────────────────────────────────────

/// Un lado de la comparación: dónde vive la tabla.
pub struct Side<'a> {
    pub handle: &'a ServerHandle,
    pub database: &'a str,
    pub schema: &'a str,
    pub table: &'a str,
    /// Dónde se anotan las conexiones que se usen, para poder cancelar la lectura desde afuera.
    pub sink: Option<CancelSink>,
}

/// Qué se compara: las columnas de la clave y las del resto, ya resueltas contra las dos tablas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub key_columns: Vec<String>,
    /// Columnas comunes que no son la clave, en el orden del origen.
    pub columns: Vec<String>,
    /// Columnas que existen de un solo lado o que calcula el servidor: no se comparan.
    pub skipped: Vec<String>,
    /// Alguna columna de la clave la genera el servidor (identidad): el `INSERT` necesita
    /// `OVERRIDING SYSTEM VALUE` para poder escribirla.
    pub key_generated: bool,
}

/// Decide qué columnas se comparan. Pura: recibe las formas ya leídas.
///
/// No adivina: sin clave en el origen, o con un destino cuya clave no coincide, es un error con el
/// motivo. Emparejar por «todas las columnas» o por posición daría informes que parecen exactos.
pub fn layout(source: &TableShape, target: &TableShape) -> Result<Layout> {
    let Some(key) = &source.key else {
        return Err(Error::Config(format!(
            "{}.{} no tiene clave primaria ni un índice único sobre columnas NOT NULL, y sin una \
             clave no hay forma de saber qué fila del origen corresponde a cuál del destino",
            source.schema, source.name
        )));
    };

    if let Some(reason) = &target.read_only {
        return Err(Error::Config(format!(
            "{}.{} no se puede igualar: {reason}",
            target.schema, target.name
        )));
    }

    let mut target_key = target.key_columns().to_vec();
    let mut wanted = key.columns.clone();
    target_key.sort();
    wanted.sort();
    if target_key != wanted {
        return Err(Error::Config(format!(
            "el destino {}.{} no tiene una clave sobre las mismas columnas que el origen ({}): los \
             UPDATE y DELETE del script podrían tocar más de una fila",
            target.schema,
            target.name,
            key.columns.join(", ")
        )));
    }

    let mut columns = Vec::new();
    let mut skipped = Vec::new();
    for column in &source.columns {
        if key.columns.contains(&column.name) {
            continue;
        }
        match target.column(&column.name) {
            Some(other) if !column.generated && !other.generated => {
                columns.push(column.name.clone())
            }
            _ => skipped.push(column.name.clone()),
        }
    }
    for column in &target.columns {
        if source.column(&column.name).is_none() {
            skipped.push(column.name.clone());
        }
    }

    Ok(Layout {
        key_generated: key
            .columns
            .iter()
            .any(|name| source.column(name).is_some_and(|c| c.generated)),
        key_columns: key.columns.clone(),
        columns,
        skipped,
    })
}

/// El SELECT de un lado: clave y valores como texto, ordenado por la clave en colación `"C"`.
pub fn select_sql(schema: &str, table: &str, layout: &Layout) -> String {
    let as_text = |name: &String| format!("({})::text", quote_ident(name));
    let selected: Vec<String> = layout
        .key_columns
        .iter()
        .chain(&layout.columns)
        .map(as_text)
        .collect();
    let order: Vec<String> = layout
        .key_columns
        .iter()
        .map(|name| format!("{} COLLATE \"C\"", as_text(name)))
        .collect();
    format!(
        "SELECT {} FROM {}.{} ORDER BY {}",
        selected.join(", "),
        quote_ident(schema),
        quote_ident(table),
        order.join(", ")
    )
}

/// Abre el cursor de un lado. El cliente se devuelve junto al flujo: tiene que vivir mientras se lo
/// recorre.
pub async fn read_side(
    side: &Side<'_>,
    layout: &Layout,
) -> Result<(
    deadpool_postgres::Object,
    impl Stream<Item = Result<KeyedRow>> + Unpin,
)> {
    let sql = select_sql(side.schema, side.table, layout);
    let open = async {
        let client = side.handle.client(side.database).await?;
        let stream = client
            .query_raw(&sql, std::iter::empty::<&str>())
            .await
            .map_err(Error::from)?;
        Ok::<_, Error>((client, stream))
    };
    let (client, stream) = match &side.sink {
        Some(sink) => conn::cancelable(sink.clone(), open).await?,
        None => open.await?,
    };

    let key_len = layout.key_columns.len();
    let rows = stream.map_err(Error::from).and_then(move |row| async move {
        let mut key = Vec::with_capacity(key_len);
        let mut values = Vec::with_capacity(row.len().saturating_sub(key_len));
        for index in 0..row.len() {
            let value: Option<String> = row.try_get(index).map_err(Error::from)?;
            if index < key_len {
                key.push(value.ok_or_else(|| {
                    Error::Config("una columna de la clave devolvió NULL".to_owned())
                })?);
            } else {
                values.push(value);
            }
        }
        Ok(KeyedRow { key, values })
    });
    Ok((client, Box::pin(rows)))
}

// ─── Script ─────────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DataAction {
    Insert,
    Update,
    Delete,
}

/// Una sentencia del script, con la clave de la fila a la que pertenece.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataStatement {
    pub action: DataAction,
    pub key: Vec<String>,
    pub risk: Risk,
    pub sql: String,
}

/// Un literal SQL. Una cadena con `\` se escribe como `E'…'` con la barra duplicada, que vale lo
/// mismo con `standard_conforming_strings` encendido o apagado: no se confía en el ajuste del
/// servidor de destino, que acá no se conoce.
pub fn literal(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "NULL".to_owned();
    };
    let quoted = value.replace('\'', "''");
    if value.contains('\\') {
        format!("E'{}'", quoted.replace('\\', "\\\\"))
    } else {
        format!("'{quoted}'")
    }
}

/// El script que lleva el destino al estado del origen. Primero lo que agrega, después lo que
/// cambia y al final lo que borra, para poder cortarlo antes de lo destructivo.
///
/// Los literales van sin cast: el servidor los resuelve contra el tipo de la columna del destino.
/// Con claves foráneas entre tablas el orden correcto entre ellas lo tiene que decidir quien corre
/// el script; acá se trata una tabla.
pub fn statements(
    schema: &str,
    table: &str,
    layout: &Layout,
    rows: &[RowDiff],
) -> Vec<DataStatement> {
    let target = format!("{}.{}", quote_ident(schema), quote_ident(table));
    let where_key = |key: &[String]| {
        layout
            .key_columns
            .iter()
            .zip(key)
            .map(|(name, value)| format!("{} = {}", quote_ident(name), literal(Some(value))))
            .collect::<Vec<_>>()
            .join(" AND ")
    };

    let mut inserts = Vec::new();
    let mut updates = Vec::new();
    let mut deletes = Vec::new();

    for row in rows {
        match row {
            RowDiff::OnlySource { key, values } => {
                let names: Vec<String> = layout
                    .key_columns
                    .iter()
                    .chain(&layout.columns)
                    .map(|name| quote_ident(name))
                    .collect();
                let literals: Vec<String> = key
                    .iter()
                    .map(|value| literal(Some(value)))
                    .chain(values.iter().map(|value| literal(value.as_deref())))
                    .collect();
                let overriding = if layout.key_generated {
                    " OVERRIDING SYSTEM VALUE"
                } else {
                    ""
                };
                inserts.push(DataStatement {
                    action: DataAction::Insert,
                    key: key.clone(),
                    risk: if layout.key_generated {
                        Risk::Review
                    } else {
                        Risk::Safe
                    },
                    sql: format!(
                        "INSERT INTO {target} ({}){overriding} VALUES ({});",
                        names.join(", "),
                        literals.join(", ")
                    ),
                });
            }
            RowDiff::Different {
                key,
                source,
                changed,
                ..
            } => {
                let sets: Vec<String> = changed
                    .iter()
                    .map(|&index| {
                        format!(
                            "{} = {}",
                            quote_ident(&layout.columns[index]),
                            literal(source[index].as_deref())
                        )
                    })
                    .collect();
                updates.push(DataStatement {
                    action: DataAction::Update,
                    key: key.clone(),
                    risk: Risk::Review,
                    sql: format!(
                        "UPDATE {target} SET {} WHERE {};",
                        sets.join(", "),
                        where_key(key)
                    ),
                });
            }
            RowDiff::OnlyTarget { key, .. } => deletes.push(DataStatement {
                action: DataAction::Delete,
                key: key.clone(),
                risk: Risk::Destructive,
                sql: format!("DELETE FROM {target} WHERE {};", where_key(key)),
            }),
        }
    }

    inserts.extend(updates);
    inserts.extend(deletes);
    inserts
}

/// El script como texto, listo para pegar en el editor.
pub fn script(statements: &[DataStatement]) -> String {
    statements
        .iter()
        .map(|statement| statement.sql.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

// ─── Todo junto ─────────────────────────────────────────────────────────────────────────────────

/// El resultado entero: informe y script salen de la misma lectura.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataComparison {
    pub source: String,
    pub target: String,
    pub layout: Layout,
    pub rows: Vec<RowDiff>,
    pub counts: Counts,
    /// Se encontraron más diferencias que `max_diffs`: el informe y el script están incompletos.
    pub truncated: bool,
    pub max_diffs: usize,
    pub statements: Vec<DataStatement>,
    pub warnings: Vec<String>,
}

/// Compara el contenido de dos tablas.
///
/// Los dos cursores se leen a la vez y no se acumulan filas: lo que ocupa memoria es el informe,
/// que tiene techo. `progress` se llama cada tanto desde esta misma tarea.
pub async fn compare(
    source: Side<'_>,
    target: Side<'_>,
    max_diffs: usize,
    progress: &(dyn Fn(Progress) + Sync),
) -> Result<DataComparison> {
    let (source_shape, target_shape) = tokio::try_join!(
        data::shape_by_name(source.handle, source.database, source.schema, source.table),
        data::shape_by_name(target.handle, target.database, target.schema, target.table),
    )?;
    let layout = layout(&source_shape, &target_shape)?;

    let ((_source_client, source_rows), (_target_client, target_rows)) =
        tokio::try_join!(read_side(&source, &layout), read_side(&target, &layout))?;

    let merged = merge(source_rows, target_rows, max_diffs, progress).await?;
    let statements = statements(target.schema, target.table, &layout, &merged.rows);

    let mut warnings = Vec::new();
    if merged.truncated {
        warnings.push(format!(
            "Hay más de {max_diffs} diferencias: el informe y el script cubren solo las primeras. \
             Los conteos son los totales."
        ));
    }
    if !layout.skipped.is_empty() {
        warnings.push(format!(
            "No se comparan: {} (existen de un solo lado o las calcula el servidor).",
            layout.skipped.join(", ")
        ));
    }
    if layout.key_generated {
        warnings.push(
            "La clave es una columna de identidad: los INSERT llevan OVERRIDING SYSTEM VALUE y \
             no mueven la secuencia del destino."
                .to_owned(),
        );
    }

    Ok(DataComparison {
        source: format!(
            "{} · {}.{}.{}",
            source.handle.profile.name, source.database, source.schema, source.table
        ),
        target: format!(
            "{} · {}.{}.{}",
            target.handle.profile.name, target.database, target.schema, target.table
        ),
        layout,
        rows: merged.rows,
        counts: merged.counts,
        truncated: merged.truncated,
        max_diffs,
        statements,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    fn row(key: &[&str], values: &[Option<&str>]) -> Result<KeyedRow> {
        Ok(KeyedRow {
            key: key.iter().map(|s| s.to_string()).collect(),
            values: values.iter().map(|v| v.map(str::to_owned)).collect(),
        })
    }

    async fn run(
        source: Vec<Result<KeyedRow>>,
        target: Vec<Result<KeyedRow>>,
        max: usize,
    ) -> Result<Merged> {
        merge(stream::iter(source), stream::iter(target), max, &|_| {}).await
    }

    fn layout_of(keys: &[&str], columns: &[&str]) -> Layout {
        Layout {
            key_columns: keys.iter().map(|s| s.to_string()).collect(),
            columns: columns.iter().map(|s| s.to_string()).collect(),
            skipped: vec![],
            key_generated: false,
        }
    }

    #[tokio::test]
    async fn dos_tablas_vacias_no_difieren() {
        let merged = run(vec![], vec![], 10).await.unwrap();
        assert_eq!(merged, Merged::default());
    }

    #[tokio::test]
    async fn filas_iguales_solo_se_cuentan() {
        let a = || vec![row(&["1"], &[Some("x")]), row(&["2"], &[None])];
        let merged = run(a(), a(), 10).await.unwrap();
        assert_eq!(merged.counts.equal, 2);
        assert!(merged.rows.is_empty());
    }

    #[tokio::test]
    async fn detecta_filas_solo_en_origen_y_solo_en_destino() {
        let source = vec![row(&["1"], &[Some("a")]), row(&["3"], &[Some("c")])];
        let target = vec![row(&["2"], &[Some("b")]), row(&["3"], &[Some("c")])];
        let merged = run(source, target, 10).await.unwrap();
        assert_eq!(merged.counts.only_source, 1);
        assert_eq!(merged.counts.only_target, 1);
        assert_eq!(merged.counts.equal, 1);
        assert!(matches!(&merged.rows[0], RowDiff::OnlySource { key, .. } if key == &["1"]));
        assert!(matches!(&merged.rows[1], RowDiff::OnlyTarget { key, .. } if key == &["2"]));
    }

    #[tokio::test]
    async fn un_lado_vacio_deja_todo_del_otro_como_diferencia() {
        let merged = run(vec![], vec![row(&["1"], &[])], 10).await.unwrap();
        assert_eq!(merged.counts.only_target, 1);
        let merged = run(vec![row(&["1"], &[])], vec![], 10).await.unwrap();
        assert_eq!(merged.counts.only_source, 1);
    }

    #[tokio::test]
    async fn marca_las_columnas_que_difieren() {
        let source = vec![row(&["1"], &[Some("a"), Some("b"), Some("c")])];
        let target = vec![row(&["1"], &[Some("a"), Some("X"), Some("Y")])];
        let merged = run(source, target, 10).await.unwrap();
        let RowDiff::Different { changed, .. } = &merged.rows[0] else {
            panic!("se esperaba una fila distinta");
        };
        assert_eq!(changed, &[1, 2]);
    }

    #[tokio::test]
    async fn null_no_es_cadena_vacia() {
        let source = vec![row(&["1"], &[None])];
        let target = vec![row(&["1"], &[Some("")])];
        let merged = run(source, target, 10).await.unwrap();
        assert_eq!(merged.counts.different, 1);

        let merged = run(vec![row(&["1"], &[None])], vec![row(&["1"], &[None])], 10)
            .await
            .unwrap();
        assert_eq!(merged.counts.equal, 1);
    }

    #[tokio::test]
    async fn claves_compuestas_se_emparejan_columna_por_columna() {
        let source = vec![
            row(&["1", "a"], &[Some("x")]),
            row(&["1", "b"], &[Some("x")]),
            row(&["2", "a"], &[Some("x")]),
        ];
        let target = vec![
            row(&["1", "b"], &[Some("x")]),
            row(&["2", "a"], &[Some("y")]),
        ];
        let merged = run(source, target, 10).await.unwrap();
        assert_eq!(merged.counts.only_source, 1);
        assert_eq!(merged.counts.equal, 1);
        assert_eq!(merged.counts.different, 1);
    }

    #[tokio::test]
    async fn el_orden_es_el_del_texto_y_no_el_numerico() {
        // «10» va antes que «9» en colación C: los dos lados piden lo mismo, así que empareja.
        let rows = || vec![row(&["10"], &[]), row(&["9"], &[])];
        let merged = run(rows(), rows(), 10).await.unwrap();
        assert_eq!(merged.counts.equal, 2);
    }

    #[tokio::test]
    async fn un_flujo_desordenado_o_con_clave_repetida_es_un_error() {
        let desordenado = vec![row(&["2"], &[]), row(&["1"], &[])];
        assert!(run(desordenado, vec![], 10).await.is_err());
        let repetido = vec![row(&["1"], &[]), row(&["1"], &[])];
        assert!(run(vec![], repetido, 10).await.is_err());
    }

    #[tokio::test]
    async fn el_error_de_lectura_se_propaga() {
        let source = vec![row(&["1"], &[]), Err(Error::Canceled)];
        let outcome = run(source, vec![], 10).await;
        assert!(matches!(outcome, Err(Error::Canceled)));
    }

    #[tokio::test]
    async fn el_tope_corta_el_informe_pero_no_el_conteo() {
        let source: Vec<_> = (0..10).map(|n| row(&[&format!("{n}")], &[])).collect();
        let merged = run(source, vec![], 3).await.unwrap();
        assert_eq!(merged.rows.len(), 3);
        assert_eq!(merged.counts.only_source, 10);
        assert!(merged.truncated);
    }

    #[test]
    fn literal_escapa_comillas_barras_y_nulos() {
        assert_eq!(literal(None), "NULL");
        assert_eq!(literal(Some("")), "''");
        assert_eq!(literal(Some("o'brien")), "'o''brien'");
        assert_eq!(literal(Some(r"a\b")), r"E'a\\b'");
        assert_eq!(literal(Some(r"it's \")), r"E'it''s \\'");
        assert_eq!(
            literal(Some("'; DROP TABLE t; --")),
            "'''; DROP TABLE t; --'"
        );
    }

    #[test]
    fn el_script_ordena_agregar_cambiar_borrar_y_marca_el_riesgo() {
        let layout = layout_of(&["id"], &["Nombre", "nota"]);
        let rows = vec![
            RowDiff::OnlyTarget {
                key: vec!["9".into()],
                values: vec![],
            },
            RowDiff::Different {
                key: vec!["2".into()],
                source: vec![Some("nuevo".into()), None],
                target: vec![Some("viejo".into()), Some("x".into())],
                changed: vec![0, 1],
            },
            RowDiff::OnlySource {
                key: vec!["1".into()],
                values: vec![Some("o'k".into()), None],
            },
        ];
        let out = statements("public", "clientes", &layout, &rows);
        let sql: Vec<&str> = out.iter().map(|s| s.sql.as_str()).collect();
        assert_eq!(
            sql,
            [
                "INSERT INTO public.clientes (id, \"Nombre\", nota) VALUES ('1', 'o''k', NULL);",
                "UPDATE public.clientes SET \"Nombre\" = 'nuevo', nota = NULL WHERE id = '2';",
                "DELETE FROM public.clientes WHERE id = '9';",
            ]
        );
        assert_eq!(out[0].risk, Risk::Safe);
        assert_eq!(out[1].risk, Risk::Review);
        assert_eq!(out[2].risk, Risk::Destructive);
    }

    #[test]
    fn clave_compuesta_en_el_where_y_solo_columnas_cambiadas() {
        let layout = layout_of(&["a", "b"], &["c", "d"]);
        let rows = vec![RowDiff::Different {
            key: vec!["1".into(), "x".into()],
            source: vec![Some("1".into()), Some("2".into())],
            target: vec![Some("1".into()), Some("3".into())],
            changed: vec![1],
        }];
        let out = statements("s", "t", &layout, &rows);
        assert_eq!(
            out[0].sql,
            "UPDATE s.t SET d = '2' WHERE a = '1' AND b = 'x';"
        );
    }

    #[test]
    fn una_clave_de_identidad_agrega_overriding() {
        let mut layout = layout_of(&["id"], &[]);
        layout.key_generated = true;
        let rows = vec![RowDiff::OnlySource {
            key: vec!["1".into()],
            values: vec![],
        }];
        let out = statements("s", "t", &layout, &rows);
        assert_eq!(
            out[0].sql,
            "INSERT INTO s.t (id) OVERRIDING SYSTEM VALUE VALUES ('1');"
        );
        assert_eq!(out[0].risk, Risk::Review);
    }

    #[test]
    fn el_select_ordena_por_el_texto_de_la_clave_en_colacion_c() {
        let layout = layout_of(&["id", "Tenant"], &["x"]);
        assert_eq!(
            select_sql("public", "t", &layout),
            "SELECT (id)::text, (\"Tenant\")::text, (x)::text FROM public.t \
             ORDER BY (id)::text COLLATE \"C\", (\"Tenant\")::text COLLATE \"C\""
        );
    }

    fn shape(columns: &[(&str, bool)], key: Option<&[&str]>) -> TableShape {
        use crate::data::{Column, Key, KeyKind};
        TableShape {
            oid: 1,
            schema: "public".into(),
            name: "t".into(),
            columns: columns
                .iter()
                .map(|(name, generated)| Column {
                    name: name.to_string(),
                    type_name: "text".into(),
                    not_null: true,
                    default: None,
                    generated: *generated,
                    comment: None,
                })
                .collect(),
            key: key.map(|cols| Key {
                name: "t_pkey".into(),
                kind: KeyKind::Primary,
                columns: cols.iter().map(|s| s.to_string()).collect(),
            }),
            read_only: None,
        }
    }

    #[test]
    fn sin_clave_en_el_origen_es_un_error_claro() {
        let source = shape(&[("a", false)], None);
        let target = shape(&[("a", false)], Some(&["a"]));
        let error = layout(&source, &target).unwrap_err().to_string();
        assert!(error.contains("no tiene clave primaria"), "{error}");
    }

    #[test]
    fn el_destino_tiene_que_tener_la_misma_clave() {
        let source = shape(&[("id", false), ("n", false)], Some(&["id"]));
        let target = shape(&[("id", false), ("n", false)], Some(&["n"]));
        assert!(layout(&source, &target).is_err());
    }

    #[test]
    fn columnas_de_un_solo_lado_y_generadas_no_se_comparan() {
        let source = shape(
            &[
                ("id", true),
                ("a", false),
                ("solo_o", false),
                ("calc", true),
            ],
            Some(&["id"]),
        );
        let target = shape(
            &[
                ("id", true),
                ("a", false),
                ("solo_d", false),
                ("calc", true),
            ],
            Some(&["id"]),
        );
        let layout = layout(&source, &target).unwrap();
        assert_eq!(layout.columns, ["a"]);
        assert_eq!(layout.skipped, ["solo_o", "calc", "solo_d"]);
        assert!(layout.key_generated);
    }
}
