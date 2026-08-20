//! Plan de ejecución.
//!
//! Se pide en `FORMAT JSON` y se convierte a un árbol propio. La conversión no es un trámite: el
//! JSON de PostgreSQL trae los tiempos *por vuelta* y acumulados con los hijos adentro, así que
//! leído tal cual señala como culpable al nodo de más arriba, que es justamente el que no lo es.
//! Acá se calcula el tiempo propio de cada nodo y se marca dónde la estimación se fue lejos, que
//! son las dos cosas que uno mira cuando abre un plan.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

use super::advice::{advise, Advice};
use super::exec::{Limits, Outcome, QuerySession};

/// A partir de acá la estimación está lo bastante lejos de la realidad como para que valga la pena
/// señalarla: es el síntoma clásico de estadísticas viejas o de un `ANALYZE` que falta.
const MISESTIMATION_RATIO: f64 = 10.0;

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainOptions {
    /// Ejecuta la consulta de verdad para medir tiempos reales.
    pub analyze: bool,
    /// Agrega el detalle de bloques leídos y encontrados en caché.
    pub buffers: bool,
    pub verbose: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub root: PlanNode,
    pub planning_ms: Option<f64>,
    pub execution_ms: Option<f64>,
    /// `true` si el plan trae medidas reales y no solo estimaciones.
    pub analyzed: bool,
    /// Lo que conviene mirar, ya leído (ver [`super::advice`]).
    pub advice: Vec<Advice>,
    /// La respuesta del servidor tal cual vino.
    ///
    /// Se conserva porque el árbol de acá es una lectura y no el dato: para pegar el plan en
    /// explain.dalibo.com o en pev2 hace falta el JSON entero, con los campos que esta aplicación
    /// no muestra. Reconstruirlo desde el árbol sería inventar la mitad.
    pub json: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanNode {
    pub node_type: String,
    pub relation: Option<String>,
    /// El esquema de esa relación. Solo llega con `VERBOSE`, y por eso el plan se pide siempre así:
    /// sin él, un `CREATE INDEX ON trabajos` sugerido podría caer en otra tabla que se llame igual
    /// en otro esquema del `search_path`.
    pub schema: Option<String>,
    pub index: Option<String>,
    /// La condición que más explica al nodo: la del índice, la del join o el filtro.
    pub condition: Option<String>,
    /// El `Filter` suelto, que en un `Index Scan` es lo que el índice **no** resolvió y se terminó
    /// mirando fila por fila. Va aparte de `condition` porque ahí gana la del índice.
    pub filter: Option<String>,

    pub startup_cost: f64,
    pub total_cost: f64,
    pub plan_rows: f64,

    pub actual_rows: Option<f64>,
    pub loops: Option<f64>,
    /// Tiempo total del nodo con sus hijos adentro, ya multiplicado por las vueltas.
    pub total_ms: Option<f64>,
    /// Tiempo del nodo sin el de sus hijos: es el que señala al culpable.
    pub self_ms: Option<f64>,
    /// Filas que el nodo descartó. Muchas acá suelen significar que falta un índice.
    pub rows_removed: Option<f64>,

    /// `true` cuando lo estimado y lo real se apartan más de [`MISESTIMATION_RATIO`] veces.
    pub misestimated: bool,

    /// Cómo ordenó (`quicksort`, `external merge`, …) y cuánto espacio le llevó, en kB.
    pub sort_method: Option<String>,
    pub sort_space_kb: Option<f64>,
    /// `true` cuando ese espacio no entró en `work_mem` y terminó escribiéndose en disco.
    pub sort_on_disk: bool,

    pub shared_hit_blocks: Option<f64>,
    pub shared_read_blocks: Option<f64>,

    pub children: Vec<PlanNode>,
}

/// Arma la sentencia de `EXPLAIN`. Es una función pura para poder verificarla sin servidor.
pub fn statement(sql: &str, options: ExplainOptions) -> String {
    let mut flags = vec!["FORMAT JSON"];
    if options.analyze {
        flags.push("ANALYZE");
    }
    if options.buffers {
        flags.push("BUFFERS");
    }
    if options.verbose {
        flags.push("VERBOSE");
    }

    format!("EXPLAIN ({}) {sql}", flags.join(", "))
}

/// Advertencia que corresponde mostrar antes de pedir el plan, o `None` si no la hay.
///
/// `EXPLAIN ANALYZE` **ejecuta** la consulta: pedirlo sobre un `DELETE` para «ver qué haría» borra
/// las filas igual. Enterarse después no sirve de nada.
pub fn warning(sql: &str, options: ExplainOptions) -> Option<&'static str> {
    const ESCRITURAS: [&str; 4] = ["INSERT", "UPDATE", "DELETE", "MERGE"];

    let primera = sql
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_uppercase();

    (options.analyze && ESCRITURAS.contains(&primera.as_str())).then_some(
        "EXPLAIN ANALYZE ejecuta la sentencia de verdad: sobre un INSERT, UPDATE o DELETE los \
         cambios quedan aplicados. Para ver el plan sin tocar los datos, pedilo sin ANALYZE, o \
         envolvelo en una transacción que después se revierta.",
    )
}

/// Pide el plan y lo devuelve ya convertido.
pub async fn explain(session: &QuerySession, sql: &str, options: ExplainOptions) -> Result<Plan> {
    let outcome = session
        .run(&statement(sql, options), Limits { max_rows: 1 })
        .await
        // El `EXPLAIN (…)` que se antepuso corre la posición del error, y quien la va a usar para
        // marcar el editor solo conoce el texto del usuario. Se descuenta acá, que es el único
        // lugar que sabe cuánto mide el prefijo.
        .map_err(|error| shift_position(error, prefix_len(options)))?;

    let Outcome::Rows { rows, .. } = &outcome else {
        return Err(Error::Config(
            "el servidor no devolvió ningún plan".to_owned(),
        ));
    };

    let json = rows
        .first()
        .and_then(|row| row.first())
        .and_then(Option::as_deref)
        .ok_or_else(|| Error::Config("el plan vino vacío".to_owned()))?;

    parse(json)
}

/// Cuántos caracteres ocupa el `EXPLAIN (…) ` que precede a la consulta.
fn prefix_len(options: ExplainOptions) -> u32 {
    // Se mide armando la sentencia vacía en vez de contando a mano, así no hay dos lugares que
    // tengan que cambiar juntos cuando se agregue una opción.
    statement("", options).chars().count() as u32
}

fn shift_position(error: Error, prefix: u32) -> Error {
    match error {
        Error::Database {
            code,
            message,
            detail,
            hint,
            position: Some(position),
        } => Error::Database {
            code,
            message,
            detail,
            hint,
            // Con base 1: el mínimo es 1, no 0.
            position: Some(position.saturating_sub(prefix).max(1)),
        },
        other => other,
    }
}

/// Convierte la respuesta de `EXPLAIN (FORMAT JSON)`.
pub fn parse(json: &str) -> Result<Plan> {
    let value: Value = serde_json::from_str(json)
        .map_err(|e| Error::Config(format!("no se pudo leer el plan: {e}")))?;

    // La respuesta es un arreglo con un único elemento; el arreglo existe porque `EXPLAIN` puede
    // describir varias sentencias, cosa que acá no pasa porque se pide de a una.
    let first = value
        .get(0)
        .or(Some(&value))
        .ok_or_else(|| Error::Config("el plan no tiene la forma esperada".to_owned()))?;

    let root = first
        .get("Plan")
        .ok_or_else(|| Error::Config("el plan no trae el nodo raíz".to_owned()))?;

    let plan = Plan {
        root: node(root),
        planning_ms: number(first, "Planning Time"),
        execution_ms: number(first, "Execution Time"),
        analyzed: root.get("Actual Total Time").is_some(),
        advice: Vec::new(),
        json: json.to_owned(),
    };

    Ok(Plan {
        advice: advise(&plan),
        ..plan
    })
}

fn node(value: &Value) -> PlanNode {
    let children: Vec<PlanNode> = value
        .get("Plans")
        .and_then(Value::as_array)
        .map(|plans| plans.iter().map(node).collect())
        .unwrap_or_default();

    // Los tiempos vienen promediados por vuelta: un nodo interno de un nested loop que corrió mil
    // veces informa lo que tardó una sola. Sin multiplicar, el plan miente por tres órdenes de
    // magnitud justo donde más importa.
    let loops = number(value, "Actual Loops");
    let total_ms =
        number(value, "Actual Total Time").map(|per_loop| per_loop * loops.unwrap_or(1.0));

    let self_ms = total_ms.map(|total| {
        let hijos: f64 = children.iter().filter_map(|child| child.total_ms).sum();
        // El resto puede dar apenas negativo por el redondeo de los promedios.
        (total - hijos).max(0.0)
    });

    let plan_rows = number(value, "Plan Rows").unwrap_or(0.0);
    let actual_rows = number(value, "Actual Rows");

    PlanNode {
        node_type: text(value, "Node Type").unwrap_or_else(|| "?".to_owned()),
        relation: text(value, "Relation Name"),
        schema: text(value, "Schema"),
        index: text(value, "Index Name"),
        condition: [
            "Index Cond",
            "Hash Cond",
            "Merge Cond",
            "Join Filter",
            "Filter",
        ]
        .into_iter()
        .find_map(|key| text(value, key)),
        filter: text(value, "Filter"),

        startup_cost: number(value, "Startup Cost").unwrap_or(0.0),
        total_cost: number(value, "Total Cost").unwrap_or(0.0),
        plan_rows,

        actual_rows,
        loops,
        total_ms,
        self_ms,
        rows_removed: ["Rows Removed by Filter", "Rows Removed by Index Recheck"]
            .into_iter()
            .find_map(|key| number(value, key)),

        misestimated: misestimated(plan_rows, actual_rows),

        sort_method: text(value, "Sort Method"),
        sort_space_kb: number(value, "Sort Space Used"),
        // El propio servidor lo dice: `Sort Space Type` es "Memory" o "Disk".
        sort_on_disk: text(value, "Sort Space Type").as_deref() == Some("Disk"),

        shared_hit_blocks: number(value, "Shared Hit Blocks"),
        shared_read_blocks: number(value, "Shared Read Blocks"),

        children,
    }
}

/// Compara lo estimado con lo real en las dos direcciones: quedarse corto y pasarse de largo
/// duelen igual, y el que se pasa por mil es el que hace elegir un nested loop imposible.
fn misestimated(plan_rows: f64, actual_rows: Option<f64>) -> bool {
    let Some(actual) = actual_rows else {
        return false;
    };

    // El piso en 1 evita que la diferencia entre 0 y 1 filas se vea como un error infinito.
    let estimadas = plan_rows.max(1.0);
    let reales = actual.max(1.0);

    (estimadas / reales).max(reales / estimadas) >= MISESTIMATION_RATIO
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_owned)
}

fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key)?.as_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plan de un nested loop con un índice adentro, tomado de la forma que devuelve PostgreSQL.
    const FIXTURE: &str = r#"
[
  {
    "Plan": {
      "Node Type": "Nested Loop",
      "Join Type": "Inner",
      "Startup Cost": 0.29,
      "Total Cost": 16.34,
      "Plan Rows": 5,
      "Plan Width": 68,
      "Actual Startup Time": 0.021,
      "Actual Total Time": 12.5,
      "Actual Rows": 500,
      "Actual Loops": 1,
      "Plans": [
        {
          "Node Type": "Seq Scan",
          "Parent Relationship": "Outer",
          "Relation Name": "clientes",
          "Alias": "c",
          "Startup Cost": 0.0,
          "Total Cost": 1.05,
          "Plan Rows": 5,
          "Plan Width": 36,
          "Actual Startup Time": 0.008,
          "Actual Total Time": 0.5,
          "Actual Rows": 5,
          "Actual Loops": 1,
          "Filter": "(estado = 'activo'::text)",
          "Rows Removed by Filter": 2,
          "Shared Hit Blocks": 1,
          "Shared Read Blocks": 0
        },
        {
          "Node Type": "Index Scan",
          "Parent Relationship": "Inner",
          "Index Name": "ventas_cliente_idx",
          "Relation Name": "ventas",
          "Alias": "v",
          "Startup Cost": 0.29,
          "Total Cost": 3.01,
          "Plan Rows": 1,
          "Plan Width": 32,
          "Actual Startup Time": 0.01,
          "Actual Total Time": 2.0,
          "Actual Rows": 100,
          "Actual Loops": 5,
          "Index Cond": "(v.cliente_id = c.id)"
        }
      ]
    },
    "Planning Time": 0.184,
    "Execution Time": 12.9,
    "Triggers": []
  }
]
"#;

    #[test]
    fn arma_la_sentencia_con_sus_opciones() {
        assert_eq!(
            statement("SELECT 1", ExplainOptions::default()),
            "EXPLAIN (FORMAT JSON) SELECT 1"
        );
        assert_eq!(
            statement(
                "SELECT 1",
                ExplainOptions {
                    analyze: true,
                    buffers: true,
                    verbose: false,
                }
            ),
            "EXPLAIN (FORMAT JSON, ANALYZE, BUFFERS) SELECT 1"
        );
    }

    #[test]
    fn avisa_antes_de_analizar_una_escritura() {
        let analyze = ExplainOptions {
            analyze: true,
            ..ExplainOptions::default()
        };

        assert!(warning("delete from clientes", analyze).is_some());
        assert!(
            warning("SELECT * FROM clientes", analyze).is_none(),
            "un SELECT no cambia nada: avisar siempre enseñaría a ignorar el aviso"
        );
        assert!(
            warning("DELETE FROM clientes", ExplainOptions::default()).is_none(),
            "sin ANALYZE no se ejecuta nada"
        );
    }

    #[test]
    fn arma_el_arbol_con_sus_tiempos() {
        let plan = parse(FIXTURE).unwrap();

        assert!(plan.analyzed);
        assert_eq!(plan.planning_ms, Some(0.184));
        assert_eq!(plan.execution_ms, Some(12.9));
        assert_eq!(plan.root.node_type, "Nested Loop");
        assert_eq!(plan.root.children.len(), 2);
    }

    #[test]
    fn multiplica_los_tiempos_por_las_vueltas() {
        let plan = parse(FIXTURE).unwrap();
        let indice = &plan.root.children[1];

        assert_eq!(
            indice.total_ms,
            Some(10.0),
            "2 ms por vuelta y 5 vueltas son 10 ms, no 2"
        );
        assert_eq!(indice.self_ms, Some(10.0), "no tiene hijos que descontar");
    }

    #[test]
    fn el_tiempo_propio_descuenta_a_los_hijos() {
        let plan = parse(FIXTURE).unwrap();

        assert_eq!(
            plan.root.self_ms,
            Some(2.0),
            "12,5 del total menos 0,5 del seq scan y 10 del index scan"
        );
    }

    #[test]
    fn marca_las_estimaciones_que_se_fueron_lejos() {
        let plan = parse(FIXTURE).unwrap();

        assert!(plan.root.misestimated, "estimó 5 filas y salieron 500");
        assert!(
            !plan.root.children[0].misestimated,
            "el seq scan estimó exacto"
        );
        assert!(
            plan.root.children[1].misestimated,
            "estimó 1 fila por vuelta y salieron 100"
        );
    }

    #[test]
    fn conserva_lo_que_explica_a_cada_nodo() {
        let plan = parse(FIXTURE).unwrap();
        let seq = &plan.root.children[0];
        let indice = &plan.root.children[1];

        assert_eq!(seq.relation.as_deref(), Some("clientes"));
        assert_eq!(seq.condition.as_deref(), Some("(estado = 'activo'::text)"));
        assert_eq!(seq.rows_removed, Some(2.0));
        assert_eq!(seq.shared_hit_blocks, Some(1.0));
        assert_eq!(indice.index.as_deref(), Some("ventas_cliente_idx"));
        assert_eq!(indice.condition.as_deref(), Some("(v.cliente_id = c.id)"));
    }

    #[test]
    fn un_plan_sin_analyze_no_inventa_medidas() {
        let json = r#"[{"Plan": {"Node Type": "Seq Scan", "Total Cost": 1.05, "Plan Rows": 5}}]"#;
        let plan = parse(json).unwrap();

        assert!(!plan.analyzed);
        assert_eq!(plan.root.total_ms, None);
        assert_eq!(plan.root.self_ms, None);
        assert!(
            !plan.root.misestimated,
            "sin filas reales no hay con qué comparar"
        );
    }

    #[test]
    fn descuenta_el_prefijo_de_la_posicion_del_error() {
        let options = ExplainOptions::default();
        assert_eq!(
            prefix_len(options),
            "EXPLAIN (FORMAT JSON) ".chars().count() as u32
        );

        // El servidor cuenta sobre `EXPLAIN (FORMAT JSON) SELECT FROM`, con base 1: el carácter 9
        // de lo que escribió el usuario es el 9 del prefijo en adelante.
        let error = Error::Database {
            code: "42601".into(),
            message: "syntax error".into(),
            detail: None,
            hint: None,
            position: Some(prefix_len(options) + 9),
        };

        let Error::Database { position, .. } = shift_position(error, prefix_len(options)) else {
            panic!("tiene que seguir siendo un error del servidor");
        };
        assert_eq!(position, Some(9));
    }

    #[test]
    fn un_json_que_no_es_un_plan_da_error() {
        assert!(parse("no soy json").is_err());
        assert!(parse(r#"[{"otra cosa": 1}]"#).is_err());
    }
}
