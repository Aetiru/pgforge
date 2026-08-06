//! Traer filas de a páginas.
//!
//! La página siguiente se pide por clave y no por `OFFSET`: `WHERE (clave) > (última vista)` usa el
//! índice y cuesta lo mismo en la fila cien que en la novecientos mil, mientras que un `OFFSET`
//! obliga al servidor a recorrer y descartar todo lo anterior en cada página.
//!
//! Como pgforge arma esta consulta, puede pedir cada columna ya convertida a texto (`"col"::text`).
//! Eso deja usar el protocolo extendido —que hace falta para pasar el cursor como parámetro— sin
//! perder lo que se ganó en `sql::exec`: los valores los sigue formateando el servidor con la
//! función de salida del tipo, así que un enum, un compuesto o un arreglo se muestran igual acá que
//! en la grilla del editor.

use serde::{Deserialize, Serialize};
use tokio_postgres::types::ToSql;

use crate::conn::ServerHandle;
use crate::ddl::quote_ident;
use crate::error::{Error, Result};

use super::shape::TableShape;

/// Filas por página. Entran de sobra en una pantalla y el scroll pide la siguiente antes de llegar
/// al final, así que subirlo solo agranda la espera de la primera.
pub const DEFAULT_PAGE_SIZE: usize = 200;

/// Desde dónde sigue la lectura.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Cursor {
    /// Valores de clave de la última fila traída.
    After { key: Vec<String> },
    /// Sin clave no hay otra forma de avanzar. Va de la mano de que esa tabla sea de solo lectura.
    Offset { rows: usize },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub columns: Vec<String>,
    /// `None` es un NULL de verdad, distinto de la cadena vacía.
    pub rows: Vec<Vec<Option<String>>>,
    /// Con qué pedir la página siguiente. `None` cuando ya no hay más.
    pub next: Option<Cursor>,
}

/// Por qué columna se ordena, cuando el usuario eligió una en el encabezado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageOrder {
    pub column: String,
    #[serde(default)]
    pub descending: bool,
}

/// Cómo se mira la tabla: con qué orden y con qué filtro.
///
/// Va al servidor y no a la grilla porque ordenar o filtrar lo ya traído responde otra pregunta:
/// sobre una tabla de un millón de filas, «las diez más recientes» no está entre las doscientas que
/// se leyeron primero.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageView {
    #[serde(default)]
    pub order: Option<PageOrder>,
    /// Predicado del `WHERE` tal como lo escribió el usuario.
    ///
    /// Va **crudo** a la consulta, como el `DEFAULT` de una columna o la expresión de un `CHECK`:
    /// no se puede parametrizar —es una expresión, no un valor—, lo valida el servidor al ejecutar,
    /// y es la misma frontera de confianza que el editor de consultas. Lo ejecuta el mismo usuario
    /// con sus mismos privilegios.
    #[serde(default)]
    pub filter: Option<String>,
}

impl PageView {
    fn predicate(&self) -> Option<&str> {
        self.filter
            .as_deref()
            .map(str::trim)
            .filter(|f| !f.is_empty())
    }
}

/// Arma el `SELECT` de una página.
///
/// Es pura para poder verificarla sin servidor: es el lugar donde un error no se nota hasta que
/// alguien pierde una fila al paginar.
pub fn select(
    shape: &TableShape,
    cursor: Option<&Cursor>,
    limit: usize,
    view: &PageView,
) -> Result<String> {
    let columns = shape
        .columns
        .iter()
        // El `::text` es lo que permite leer cualquier tipo sin conocerlo.
        .map(|column| format!("{}::text", quote_ident(&column.name)))
        .collect::<Vec<_>>()
        .join(", ");

    let table = format!(
        "{}.{}",
        quote_ident(&shape.schema),
        quote_ident(&shape.name)
    );

    let key = shape.key_columns();
    // El alias importa, no es cosmético: `"id"::text` sin alias explícito queda nombrada `id` en
    // la salida, igual que la columna de origen. Un `ORDER BY id` sin calificar se ata a esa
    // columna de salida — ya casteada a texto — y termina ordenando alfabéticamente en vez de por
    // el tipo real (`10` antes que `9`). Calificar con `t.` fuerza la referencia a la tabla, que es
    // lo único que ve un `WHERE` de todos modos; acá se calfica también por prolijidad y para que
    // no dependa de esa asimetría entre cláusulas.
    let mut sql = format!("SELECT {columns} FROM {table} AS t");

    let mut conditions: Vec<String> = Vec::new();

    if let Some(filter) = view.predicate() {
        // Entre paréntesis: un filtro con `OR` adentro, pegado al `AND` del cursor, cambiaría de
        // significado y devolvería filas que el usuario no pidió.
        conditions.push(format!("({filter})"));
    }

    match cursor {
        Some(Cursor::After { key: values }) => {
            if key.is_empty() {
                return Err(Error::Config(
                    "no se puede paginar por clave una tabla que no la tiene".to_owned(),
                ));
            }
            if values.len() != key.len() {
                return Err(Error::Config(format!(
                    "el cursor trae {} valores y la clave tiene {}",
                    values.len(),
                    key.len()
                )));
            }
            // El cursor por clave solo vale con el orden de la clave: con otro orden, «lo que sigue
            // después de esta clave» no es lo que sigue en pantalla. Ahí se pagina por posición.
            if view.order.is_some() {
                return Err(Error::Config(
                    "con un orden elegido, la página siguiente se pide por posición".to_owned(),
                ));
            }

            // La comparación va por fila entera y no columna por columna: con una clave compuesta,
            // `a > $1 AND b > $2` se saltea filas legítimas —las que tienen el mismo `a` y un `b`
            // menor— y además no puede aprovechar el índice.
            let names = join(key.iter().map(|name| format!("t.{}", quote_ident(name))));
            let placeholders = join(
                key.iter()
                    .enumerate()
                    .map(|(index, name)| cast_param(index + 1, shape, name)),
            );
            conditions.push(format!("({names}) > ({placeholders})"));
        }
        Some(Cursor::Offset { .. }) | None => {}
    }

    if !conditions.is_empty() {
        sql.push_str(&format!(" WHERE {}", conditions.join(" AND ")));
    }

    // Sin un orden estable la paginación no significa nada: el servidor puede devolver las mismas
    // filas en otro orden y una fila aparecería dos veces o ninguna. La clave va como desempate
    // incluso con un orden elegido, porque ordenar por una columna repetida no define nada.
    let mut order: Vec<String> = Vec::new();

    if let Some(chosen) = &view.order {
        if shape.column(&chosen.column).is_none() {
            return Err(Error::Config(format!(
                "la tabla no tiene una columna llamada «{}»",
                chosen.column
            )));
        }
        order.push(format!(
            "t.{}{}",
            quote_ident(&chosen.column),
            if chosen.descending { " DESC" } else { "" }
        ));
    }

    for name in key.iter() {
        // La columna elegida ya está en la lista: repetirla como desempate no desempata nada.
        if view
            .order
            .as_ref()
            .is_some_and(|chosen| &chosen.column == name)
        {
            continue;
        }
        order.push(format!("t.{}", quote_ident(name)));
    }

    if !order.is_empty() {
        sql.push_str(&format!(" ORDER BY {}", order.join(", ")));
    }

    sql.push_str(&format!(" LIMIT {limit}"));

    if let Some(Cursor::Offset { rows }) = cursor {
        sql.push_str(&format!(" OFFSET {rows}"));
    }

    Ok(sql)
}

/// `$n::text::tipo`.
///
/// El doble casteo es a propósito: escribir `$1::bigint` haría que PostgreSQL infiera el parámetro
/// como `bigint` y el cliente fallaría al codificar un texto. Pasando por `text` el servidor
/// convierte con la misma función de entrada que usaría en una sentencia escrita a mano, con lo
/// cual sirve para todo tipo que exista, incluidos los de una extensión.
fn cast_param(position: usize, shape: &TableShape, column: &str) -> String {
    match shape.column(column) {
        Some(column) => format!("${position}::text::{}", column.type_name),
        // No debería pasar: la clave sale del mismo catálogo que las columnas.
        None => format!("${position}::text"),
    }
}

fn join(parts: impl Iterator<Item = String>) -> String {
    parts.collect::<Vec<_>>().join(", ")
}

/// Trae una página de filas.
pub async fn page(
    handle: &ServerHandle,
    database: &str,
    shape: &TableShape,
    cursor: Option<&Cursor>,
    limit: usize,
    view: &PageView,
) -> Result<Page> {
    let sql = select(shape, cursor, limit, view)?;
    let client = handle.client(database).await?;

    let values: Vec<String> = match cursor {
        Some(Cursor::After { key }) => key.clone(),
        _ => Vec::new(),
    };
    let params: Vec<&(dyn ToSql + Sync)> = values
        .iter()
        .map(|value| value as &(dyn ToSql + Sync))
        .collect();

    let rows = client.query(sql.as_str(), &params).await?;
    let rows: Vec<Vec<Option<String>>> = rows
        .iter()
        .map(|row| (0..row.len()).map(|index| row.get(index)).collect())
        .collect();

    // Una página incompleta significa que no hay más: pedir otra devolvería vacío y sería un viaje
    // al servidor para enterarse de nada.
    let next = (rows.len() == limit)
        .then(|| next_cursor(shape, cursor, &rows, view))
        .flatten();

    Ok(Page {
        columns: shape.columns.iter().map(|c| c.name.clone()).collect(),
        rows,
        next,
    })
}

fn next_cursor(
    shape: &TableShape,
    cursor: Option<&Cursor>,
    rows: &[Vec<Option<String>>],
    view: &PageView,
) -> Option<Cursor> {
    let key = shape.key_columns();

    // Con un orden elegido, «lo que sigue después de esta clave» ya no es lo que sigue en pantalla:
    // se paga el `OFFSET` porque es lo único que respeta ese orden.
    if key.is_empty() || view.order.is_some() {
        let seen = match cursor {
            Some(Cursor::Offset { rows }) => *rows,
            _ => 0,
        };
        return Some(Cursor::Offset {
            rows: seen + rows.len(),
        });
    }

    // Los valores de clave de la última fila. Las columnas de una clave son NOT NULL, así que un
    // `None` acá significaría que la fila no se puede señalar y seguir sería inventar.
    let last = rows.last()?;
    let values = key
        .iter()
        .map(|name| {
            let index = shape.columns.iter().position(|c| &c.name == name)?;
            last.get(index)?.clone()
        })
        .collect::<Option<Vec<String>>>()?;

    Some(Cursor::After { key: values })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::shape::{Column, Key, KeyKind};

    fn column(name: &str, type_name: &str) -> Column {
        Column {
            name: name.into(),
            type_name: type_name.into(),
            not_null: true,
            default: None,
            generated: false,
            comment: None,
        }
    }

    fn shape(key: Option<Key>) -> TableShape {
        TableShape {
            oid: 1,
            schema: "public".into(),
            name: "clientes".into(),
            columns: vec![
                column("id", "bigint"),
                column("nombre", "text"),
                column("creado", "timestamp with time zone"),
            ],
            read_only: key.is_none().then(|| "sin clave".to_owned()),
            key,
        }
    }

    fn con_clave() -> TableShape {
        shape(Some(Key {
            name: "clientes_pkey".into(),
            kind: KeyKind::Primary,
            columns: vec!["id".into()],
        }))
    }

    fn con_clave_compuesta() -> TableShape {
        let mut shape = con_clave();
        shape.key = Some(Key {
            name: "clientes_pkey".into(),
            kind: KeyKind::Primary,
            columns: vec!["id".into(), "nombre".into()],
        });
        shape
    }

    #[test]
    fn la_primera_pagina_ordena_por_clave() {
        // Los identificadores van sin comillas porque `quote_ident` solo cita lo que lo necesita.
        let sql = select(&con_clave(), None, 200, &PageView::default()).unwrap();
        assert_eq!(
            sql,
            "SELECT id::text, nombre::text, creado::text \
             FROM public.clientes AS t ORDER BY t.id LIMIT 200"
        );
    }

    #[test]
    fn el_order_by_se_califica_para_no_atarse_a_la_columna_ya_casteada() {
        // Sin el alias `t.`, `ORDER BY id` se ataría a la salida `id::text` (que Postgres nombra
        // igual que la columna de origen) y ordenaría alfabéticamente en vez de por el tipo real.
        // Es justo el bug que este test existe para no dejar volver.
        let sql = select(&con_clave(), None, 200, &PageView::default()).unwrap();
        assert!(sql.contains("FROM public.clientes AS t"), "{sql}");
        assert!(sql.contains("ORDER BY t.id"), "{sql}");
        assert!(!sql.contains("ORDER BY id"), "{sql}");
    }

    #[test]
    fn la_pagina_siguiente_compara_contra_la_ultima_fila() {
        let cursor = Cursor::After {
            key: vec!["1240".into()],
        };
        let sql = select(&con_clave(), Some(&cursor), 200, &PageView::default()).unwrap();

        assert!(sql.contains("WHERE (t.id) > ($1::text::bigint)"), "{sql}");
        assert!(sql.contains("ORDER BY t.id"), "{sql}");
        assert!(!sql.contains("OFFSET"), "el keyset no usa OFFSET: {sql}");
    }

    #[test]
    fn la_clave_compuesta_compara_por_fila_entera() {
        let cursor = Cursor::After {
            key: vec!["1240".into(), "ana".into()],
        };
        let sql = select(
            &con_clave_compuesta(),
            Some(&cursor),
            50,
            &PageView::default(),
        )
        .unwrap();

        assert!(
            sql.contains("WHERE (t.id, t.nombre) > ($1::text::bigint, $2::text::text)"),
            "comparar columna por columna se saltea filas con el mismo id: {sql}"
        );
        assert!(sql.contains("ORDER BY t.id, t.nombre"), "{sql}");
    }

    #[test]
    fn sin_clave_se_pagina_por_offset() {
        let cursor = Cursor::Offset { rows: 400 };
        let sql = select(&shape(None), Some(&cursor), 200, &PageView::default()).unwrap();

        assert!(sql.ends_with("LIMIT 200 OFFSET 400"), "{sql}");
        assert!(
            !sql.contains("ORDER BY"),
            "sin clave no hay orden estable que imponer: {sql}"
        );
    }

    #[test]
    fn sin_clave_no_se_puede_paginar_por_clave() {
        let cursor = Cursor::After {
            key: vec!["1".into()],
        };
        assert!(select(&shape(None), Some(&cursor), 200, &PageView::default()).is_err());
    }

    #[test]
    fn un_cursor_con_otra_cantidad_de_valores_no_se_acepta() {
        let cursor = Cursor::After {
            key: vec!["1".into()],
        };
        let error = select(
            &con_clave_compuesta(),
            Some(&cursor),
            200,
            &PageView::default(),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("1 valores"),
            "el error tiene que decir qué no cuadra: {error}"
        );
    }

    #[test]
    fn cita_los_identificadores_que_lo_necesitan() {
        let mut shape = con_clave();
        shape.schema = "mi esquema".into();
        shape.name = "Clientes".into();
        shape.columns[0].name = "id de cliente".into();
        shape.key = Some(Key {
            name: "pk".into(),
            kind: KeyKind::Primary,
            columns: vec!["id de cliente".into()],
        });

        let sql = select(&shape, None, 10, &PageView::default()).unwrap();
        assert!(sql.contains("\"mi esquema\".\"Clientes\""), "{sql}");
        assert!(sql.contains("\"id de cliente\"::text"), "{sql}");
    }

    #[test]
    fn el_cursor_siguiente_sale_de_la_ultima_fila() {
        let shape = con_clave_compuesta();
        let rows = vec![
            vec![Some("1".into()), Some("ana".into()), None],
            vec![Some("2".into()), Some("beto".into()), None],
        ];

        assert_eq!(
            next_cursor(&shape, None, &rows, &PageView::default()),
            Some(Cursor::After {
                key: vec!["2".into(), "beto".into()]
            })
        );
    }

    fn ordenado_por(column: &str, descending: bool) -> PageView {
        PageView {
            order: Some(PageOrder {
                column: column.into(),
                descending,
            }),
            filter: None,
        }
    }

    #[test]
    fn el_orden_elegido_va_primero_y_la_clave_desempata() {
        let sql = select(&con_clave(), None, 200, &ordenado_por("nombre", true)).unwrap();
        assert!(sql.contains("ORDER BY t.nombre DESC, t.id"), "{sql}");
    }

    #[test]
    fn ordenar_por_la_misma_clave_no_la_repite() {
        let sql = select(&con_clave(), None, 200, &ordenado_por("id", false)).unwrap();
        assert!(sql.contains("ORDER BY t.id LIMIT"), "{sql}");
    }

    #[test]
    fn ordenar_por_una_columna_que_no_existe_se_rechaza() {
        // La columna llega de la interfaz: si no se valida, termina interpolada en el SQL.
        let error = select(&con_clave(), None, 200, &ordenado_por("apellido", false)).unwrap_err();
        assert!(error.to_string().contains("apellido"), "{error}");
    }

    #[test]
    fn con_un_orden_elegido_no_se_pagina_por_clave() {
        // El cursor por clave significa «lo que sigue de esta clave»: con otro orden eso no es lo
        // que sigue en pantalla, y las filas se repetirían o faltarían.
        let cursor = Cursor::After {
            key: vec!["10".into()],
        };
        assert!(select(
            &con_clave(),
            Some(&cursor),
            200,
            &ordenado_por("nombre", false)
        )
        .is_err());

        let rows = vec![vec![Some("1".into()), Some("ana".into()), None]];
        assert_eq!(
            next_cursor(&con_clave(), None, &rows, &ordenado_por("nombre", false)),
            Some(Cursor::Offset { rows: 1 })
        );
    }

    #[test]
    fn el_filtro_va_entre_parentesis_y_se_combina_con_el_cursor() {
        let view = PageView {
            order: None,
            filter: Some("nombre ILIKE 'a%' OR id < 10".into()),
        };
        let cursor = Cursor::After {
            key: vec!["3".into()],
        };
        let sql = select(&con_clave(), Some(&cursor), 200, &view).unwrap();

        // Sin los paréntesis, el `OR` se llevaría puesta la condición del cursor y la página
        // siguiente traería filas ya vistas.
        assert!(
            sql.contains("WHERE (nombre ILIKE 'a%' OR id < 10) AND (t.id) > ($1::text::bigint)"),
            "{sql}"
        );
    }

    #[test]
    fn un_filtro_en_blanco_es_no_filtrar() {
        let view = PageView {
            order: None,
            filter: Some("   ".into()),
        };
        let sql = select(&con_clave(), None, 200, &view).unwrap();
        assert!(!sql.contains("WHERE"), "{sql}");
    }

    #[test]
    fn el_offset_siguiente_acumula_lo_ya_visto() {
        let rows = vec![vec![Some("1".into()), None, None]];
        assert_eq!(
            next_cursor(
                &shape(None),
                Some(&Cursor::Offset { rows: 400 }),
                &rows,
                &PageView::default()
            ),
            Some(Cursor::Offset { rows: 401 })
        );
    }
}
