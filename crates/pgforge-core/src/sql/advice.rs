//! Qué mirar de un plan de ejecución, y qué índice propondría uno al verlo.
//!
//! Es lectura del árbol que ya devolvió [`super::explain`], sin tocar la red: las mismas cosas que
//! uno busca a ojo cuando abre un plan largo —un `Seq Scan` que descarta casi todo lo que leyó, una
//! estimación que se fue por mil, un `Sort` que terminó en disco— pero dichas en voz alta y con la
//! sentencia escrita al lado.
//!
//! **No aplica nada.** Cada sugerencia es un texto y, cuando se puede escribir, un `CREATE INDEX`
//! para copiar: crear un índice cambia el costo de cada escritura de esa tabla y esa decisión no la
//! toma una heurística. Misma regla que la comparación de esquemas, que también termina en un script.
//!
//! Las reglas son deliberadamente pocas y con umbral alto. Una lista de veinte avisos tibios se
//! ignora entera, y un índice de más se paga en cada `INSERT` para siempre.

use serde::Serialize;

use super::explain::{Plan, PlanNode};

/// Menos filas descartadas que esto no justifican un índice: leer la tabla entera de algo chico
/// cuesta menos que mantener el índice en cada escritura.
const ROWS_REMOVED_MIN: f64 = 1_000.0;

/// Y el filtro tiene que descartar la enorme mayoría de lo que leyó. Un índice que devuelve la
/// mitad de la tabla no se usa: el planificador prefiere el `Seq Scan`, y con razón.
const DISCARDED_MIN: f64 = 0.9;

/// Tope de columnas de un índice propuesto. Más que esto deja de ser una sugerencia y pasa a ser
/// una transcripción del `WHERE`.
const MAX_COLUMNS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// Está costando tiempo ahora mismo.
    Warn,
    /// Conviene saberlo, pero puede ser lo correcto para esta consulta.
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AdviceKind {
    /// Se leyó la tabla entera para quedarse con unas pocas filas.
    MissingIndex,
    /// Hay índice, pero el filtro que sobra se resuelve mirando las filas una por una.
    IndexFilter,
    /// Lo estimado y lo real se apartan tanto que el plan se eligió con datos viejos.
    StaleStats,
    /// La operación no entró en memoria y terminó en disco.
    WorkMem,
}

/// Sobre qué tabla y por qué columnas se propone el índice.
///
/// Va aparte del texto del `sql` para que la interfaz pueda abrir su diálogo de índices con esto
/// puesto en vez de obligar a copiar, pegar y volver a tipear lo mismo. Sin esquema no se llena: un
/// `CREATE INDEX` sin calificar puede caer sobre otra tabla que se llame igual.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexTarget {
    pub schema: String,
    pub table: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Advice {
    pub kind: AdviceKind,
    pub severity: Severity,
    /// Qué se vio, en una línea.
    pub title: String,
    /// Por qué importa, con los números que lo sostienen.
    pub detail: String,
    /// La sentencia que uno escribiría al ver esto, o `None` cuando no hay una sola obvia.
    pub sql: Option<String>,
    /// El índice propuesto, ya desarmado en tabla y columnas.
    pub index: Option<IndexTarget>,
}

/// Lee el plan entero y devuelve lo que vale la pena decir.
///
/// Sin `ANALYZE` la lista viene casi siempre vacía, y es correcto: un `Seq Scan` estimado no dice
/// cuántas filas se van a descartar de verdad, que es justo el número que decide si falta un índice.
pub fn advise(plan: &Plan) -> Vec<Advice> {
    let mut out = Vec::new();
    walk(&plan.root, &mut out);

    // Lo que cuesta tiempo primero; dentro de cada grupo, el orden en que aparece en el plan, que es
    // el orden en que se ejecutó.
    out.sort_by_key(|advice| match advice.severity {
        Severity::Warn => 0,
        Severity::Info => 1,
    });
    out
}

fn walk(node: &PlanNode, out: &mut Vec<Advice>) {
    if let Some(advice) = scan_advice(node) {
        push_unique(out, advice);
    }
    if let Some(advice) = index_filter_advice(node) {
        push_unique(out, advice);
    }
    if let Some(advice) = stats_advice(node) {
        push_unique(out, advice);
    }
    if let Some(advice) = sort_advice(node) {
        push_unique(out, advice);
    }

    for child in &node.children {
        walk(child, out);
    }
}

/// Un plan con veinte particiones repite el mismo aviso veinte veces; se dice una sola.
fn push_unique(out: &mut Vec<Advice>, advice: Advice) {
    if !out
        .iter()
        .any(|item| item.kind == advice.kind && item.title == advice.title)
    {
        out.push(advice);
    }
}

/// El nombre con el que se puede escribir la sentencia: calificado cuando el plan trae el esquema.
///
/// Sin calificar, un `CREATE INDEX ON trabajos` copiado a otra sesión puede terminar creando el
/// índice sobre otra tabla que se llame igual en otro esquema del `search_path`.
fn qualified(node: &PlanNode) -> Option<String> {
    let relation = node.relation.as_deref()?;
    Some(match node.schema.as_deref() {
        Some(schema) => format!("{schema}.{relation}"),
        None => relation.to_owned(),
    })
}

/// Filas leídas y tiradas por el filtro del nodo, y qué proporción del total fueron.
fn discarded(node: &PlanNode) -> Option<(f64, f64)> {
    let removed = node.rows_removed?;
    let kept = node.actual_rows?;
    let read = removed + kept;
    (read > 0.0).then(|| (removed, removed / read))
}

/// La tabla y las columnas del índice propuesto, cuando el plan trae el esquema.
fn index_target(node: &PlanNode, columns: &[String]) -> Option<IndexTarget> {
    if columns.is_empty() {
        return None;
    }
    Some(IndexTarget {
        schema: node.schema.clone()?,
        table: node.relation.clone()?,
        columns: columns.to_vec(),
    })
}

fn scan_advice(node: &PlanNode) -> Option<Advice> {
    if node.node_type != "Seq Scan" {
        return None;
    }
    let relation = qualified(node)?;
    let condition = node.filter.as_deref()?;
    let (removed, ratio) = discarded(node)?;
    if removed < ROWS_REMOVED_MIN || ratio < DISCARDED_MIN {
        return None;
    }

    let columns = filter_columns(condition);
    Some(Advice {
        kind: AdviceKind::MissingIndex,
        severity: Severity::Warn,
        title: format!(
            "Se recorrió {relation} entera para descartar {} filas",
            miles(removed)
        ),
        detail: format!(
            "El filtro «{condition}» tiró el {} % de lo que se leyó. Un índice por esas columnas \
             deja que el servidor busque en vez de recorrer.",
            porcentaje(ratio)
        ),
        sql: index_sql(&relation, &columns),
        index: index_target(node, &columns),
    })
}

/// El índice encontró las filas, pero encima quedó un filtro que se resolvió mirándolas una por
/// una. Pasa cuando la consulta filtra por una columna que el índice no incluye.
fn index_filter_advice(node: &PlanNode) -> Option<Advice> {
    if !matches!(node.node_type.as_str(), "Index Scan" | "Bitmap Heap Scan") {
        return None;
    }
    let relation = qualified(node)?;
    let filter = node.filter.as_deref()?;
    let (removed, ratio) = discarded(node)?;
    if removed < ROWS_REMOVED_MIN || ratio < DISCARDED_MIN {
        return None;
    }

    let columns = filter_columns(filter);
    let index = node.index.as_deref().unwrap_or("el índice");
    Some(Advice {
        kind: AdviceKind::IndexFilter,
        severity: Severity::Warn,
        title: format!("{index} no alcanzó: se descartaron {} filas después de usarlo", miles(removed)),
        detail: format!(
            "«{filter}» quedó fuera del índice y se resolvió leyendo cada fila. Lo que corresponde              es agregarle esas columnas al índice que ya existe, en vez de sumar uno nuevo al lado.",
        ),
        sql: index_sql(&relation, &columns),
        index: index_target(node, &columns),
    })
}

fn stats_advice(node: &PlanNode) -> Option<Advice> {
    if !node.misestimated {
        return None;
    }
    let relation = qualified(node)?;
    let actual = node.actual_rows?;

    Some(Advice {
        kind: AdviceKind::StaleStats,
        severity: Severity::Warn,
        title: format!("La estimación sobre {relation} no se parece a lo que pasó"),
        detail: format!(
            "El planificador esperaba {} filas y salieron {}. Con esa diferencia elige mal el tipo \
             de join y el orden, así que el plan de al lado no es el que correspondía.",
            miles(node.plan_rows),
            miles(actual)
        ),
        sql: Some(format!("ANALYZE {relation};")),
        index: None,
    })
}

fn sort_advice(node: &PlanNode) -> Option<Advice> {
    if !node.sort_on_disk {
        return None;
    }
    let usado = node.sort_space_kb.unwrap_or(0.0);

    Some(Advice {
        kind: AdviceKind::WorkMem,
        severity: Severity::Warn,
        title: format!("El {} terminó en disco", node.node_type),
        detail: format!(
            "Ordenar necesitó {} y no entró en `work_mem`, así que se escribió en disco{}. Subir \
             `work_mem` en esta sesión alcanza para probar si es lo que duele.",
            kilobytes(usado),
            node.sort_method
                .as_deref()
                .map(|method| format!(" ({method})"))
                .unwrap_or_default()
        ),
        // A nivel de sesión y no del servidor: `work_mem` se reserva por operación y por conexión,
        // así que subirlo para todos es la forma clásica de quedarse sin memoria.
        sql: Some(format!("SET work_mem = '{}MB';", work_mem_mb(usado))),
        index: None,
    })
}

/// El `work_mem` que hace falta, redondeado hacia arriba a una potencia de dos y con un poco de
/// aire: el sort necesita algo más que el espacio que ocupó en disco.
fn work_mem_mb(kb: f64) -> u32 {
    let mut mb = 4u32;
    let necesario = (kb / 1024.0).ceil().max(1.0) as u32;
    while mb < necesario.saturating_mul(2) && mb < 1024 {
        mb *= 2;
    }
    mb
}

/// El `CREATE INDEX` que corresponde, o `None` si del filtro no salió ninguna columna clara.
fn index_sql(relation: &str, columns: &[String]) -> Option<String> {
    if columns.is_empty() {
        return None;
    }
    // Sin nombre: que lo ponga PostgreSQL. Inventar uno acá obliga a acertarle a la convención de
    // cada proyecto, y el nombre no es lo que se está sugiriendo.
    Some(format!(
        "CREATE INDEX ON {relation} ({});",
        columns.join(", ")
    ))
}

/// Las columnas por las que filtra una condición del plan, en el orden en que conviene indexarlas.
///
/// El texto que devuelve PostgreSQL trae los `cast` y los paréntesis de su propia reconstrucción
/// —`((estado)::text = 'activo'::text)`—, así que se lee a mano en vez de partir por `=`. Lo que
/// está adentro de una función (`lower(nombre) = …`) se saltea: eso pide un índice por expresión y
/// no por columna, y proponer el equivocado es peor que no proponer nada.
pub fn filter_columns(condition: &str) -> Vec<String> {
    let bytes: Vec<char> = condition.chars().collect();
    let mut equality: Vec<String> = Vec::new();
    let mut ranges: Vec<String> = Vec::new();
    let mut depth_of_call: Vec<bool> = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        let c = bytes[i];

        if c == '(' {
            depth_of_call.push(false);
            i += 1;
            continue;
        }
        if c == ')' {
            depth_of_call.pop();
            i += 1;
            continue;
        }
        if c == '\'' {
            i += 1;
            while i < bytes.len() && bytes[i] != '\'' {
                i += 1;
            }
            i += 1;
            continue;
        }
        if !is_ident_start(c) {
            i += 1;
            continue;
        }

        let start = i;
        while i < bytes.len() && is_ident_char(bytes[i]) {
            i += 1;
        }
        let word: String = bytes[start..i].iter().collect();

        // `f(` abre una llamada: lo de adentro es una expresión, no una columna.
        if bytes.get(i) == Some(&'(') {
            depth_of_call.push(true);
            i += 1;
            continue;
        }

        if depth_of_call.iter().any(|is_call| *is_call) {
            continue;
        }

        // Un nombre calificado se indexa por su última parte: `t.estado` es la columna `estado`.
        let column = word.rsplit('.').next().unwrap_or(&word).to_owned();
        if is_keyword(&column) {
            continue;
        }

        if let Some((operator, at)) = operator_after(&bytes, i) {
            match operator {
                Operator::Equality => remember(&mut equality, column),
                Operator::Range => remember(&mut ranges, column),
            }
            // Se salta el operador para no volver a leer lo que hay en el medio: el `text` de
            // `(estado)::text = …` es el nombre de un tipo y no una columna más.
            i = at + 1;
        }
    }

    // Igualdad antes que rango: con un índice compuesto, la primera desigualdad corta el uso de las
    // columnas que siguen.
    equality.extend(ranges);
    equality.truncate(MAX_COLUMNS);
    equality
}

enum Operator {
    Equality,
    Range,
}

/// Qué comparación viene después de la columna y dónde está, salteando los `cast` y los paréntesis
/// que cierran lo que PostgreSQL abrió alrededor del nombre.
fn operator_after(chars: &[char], from: usize) -> Option<(Operator, usize)> {
    let mut i = from;
    loop {
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        // `::tipo`, incluso compuesto («character varying»).
        if chars.get(i) == Some(&':') && chars.get(i + 1) == Some(&':') {
            i += 2;
            while i < chars.len() && (is_ident_char(chars[i]) || chars[i] == ' ') {
                i += 1;
            }
            continue;
        }
        if chars.get(i) == Some(&')') {
            i += 1;
            continue;
        }
        break;
    }

    match chars.get(i)? {
        '=' => Some((Operator::Equality, i)),
        // `<>` es «distinto de»: no lo resuelve un índice.
        '<' if chars.get(i + 1) == Some(&'>') => None,
        '<' | '>' => Some((Operator::Range, i)),
        '~' => Some((Operator::Range, i)),
        _ => None,
    }
}

fn remember(list: &mut Vec<String>, column: String) {
    if !list.contains(&column) {
        list.push(column);
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '"'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.' || c == '$' || c == '"'
}

/// Palabras del propio texto del plan que no son columnas.
fn is_keyword(word: &str) -> bool {
    matches!(
        word.to_uppercase().as_str(),
        "AND" | "OR" | "NOT" | "ANY" | "ALL" | "IS" | "NULL" | "TRUE" | "FALSE" | "IN"
    )
}

fn miles(value: f64) -> String {
    let entero = value.round() as i64;
    let texto = entero.abs().to_string();
    let mut salida = String::new();
    for (index, digito) in texto.chars().enumerate() {
        if index > 0 && (texto.len() - index) % 3 == 0 {
            salida.push('.');
        }
        salida.push(digito);
    }
    if entero < 0 {
        format!("-{salida}")
    } else {
        salida
    }
}

fn porcentaje(ratio: f64) -> String {
    format!("{:.1}", ratio * 100.0)
}

fn kilobytes(kb: f64) -> String {
    if kb >= 1024.0 {
        format!("{:.1} MB", kb / 1024.0)
    } else {
        format!("{kb:.0} kB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::explain;

    fn plan(json: &str) -> Plan {
        explain::parse(json).expect("el plan de prueba tiene que leerse")
    }

    #[test]
    fn propone_un_indice_cuando_el_recorrido_completo_descarta_casi_todo() {
        let advice = advise(&plan(
            r#"[{"Plan": {
                "Node Type": "Seq Scan",
                "Relation Name": "public.trabajos",
                "Filter": "(estado = 'activo'::text)",
                "Rows Removed by Filter": 214882,
                "Actual Rows": 118,
                "Actual Total Time": 38.4,
                "Actual Loops": 1,
                "Plan Rows": 120,
                "Total Cost": 4521.0
            }}]"#,
        ));

        assert_eq!(advice.len(), 1);
        assert_eq!(advice[0].kind, AdviceKind::MissingIndex);
        assert_eq!(
            advice[0].sql.as_deref(),
            Some("CREATE INDEX ON public.trabajos (estado);")
        );
    }

    #[test]
    fn no_propone_nada_cuando_el_filtro_deja_pasar_la_mitad() {
        let advice = advise(&plan(
            r#"[{"Plan": {
                "Node Type": "Seq Scan",
                "Relation Name": "trabajos",
                "Filter": "(estado = 'activo'::text)",
                "Rows Removed by Filter": 5000,
                "Actual Rows": 5000,
                "Actual Total Time": 12.0,
                "Actual Loops": 1,
                "Plan Rows": 5000,
                "Total Cost": 100.0
            }}]"#,
        ));

        assert!(advice.is_empty());
    }

    #[test]
    fn no_propone_nada_sobre_una_tabla_chica() {
        let advice = advise(&plan(
            r#"[{"Plan": {
                "Node Type": "Seq Scan",
                "Relation Name": "estados",
                "Filter": "(activo = true)",
                "Rows Removed by Filter": 40,
                "Actual Rows": 1,
                "Actual Total Time": 0.1,
                "Actual Loops": 1,
                "Plan Rows": 1,
                "Total Cost": 1.5
            }}]"#,
        ));

        assert!(advice.is_empty());
    }

    #[test]
    fn un_plan_estimado_sin_medir_no_sugiere_indices() {
        // Sin ANALYZE no hay filas descartadas que mirar, y estimarlas sería inventar.
        let advice = advise(&plan(
            r#"[{"Plan": {
                "Node Type": "Seq Scan",
                "Relation Name": "trabajos",
                "Filter": "(estado = 'activo'::text)",
                "Plan Rows": 120,
                "Total Cost": 4521.0
            }}]"#,
        ));

        assert!(advice.is_empty());
    }

    #[test]
    fn pide_analyze_cuando_lo_estimado_y_lo_real_no_se_parecen() {
        let advice = advise(&plan(
            r#"[{"Plan": {
                "Node Type": "Index Scan",
                "Relation Name": "public.pedidos",
                "Index Name": "pedidos_pkey",
                "Plan Rows": 3,
                "Actual Rows": 90000,
                "Actual Total Time": 210.0,
                "Actual Loops": 1,
                "Total Cost": 8.4
            }}]"#,
        ));

        assert_eq!(advice.len(), 1);
        assert_eq!(advice[0].kind, AdviceKind::StaleStats);
        assert_eq!(advice[0].sql.as_deref(), Some("ANALYZE public.pedidos;"));
    }

    #[test]
    fn avisa_del_sort_que_se_fue_a_disco_con_el_work_mem_que_haria_falta() {
        let advice = advise(&plan(
            r#"[{"Plan": {
                "Node Type": "Sort",
                "Sort Method": "external merge",
                "Sort Space Used": 43008,
                "Sort Space Type": "Disk",
                "Plan Rows": 100000,
                "Actual Rows": 100000,
                "Actual Total Time": 900.0,
                "Actual Loops": 1,
                "Total Cost": 12000.0
            }}]"#,
        ));

        assert_eq!(advice.len(), 1);
        assert_eq!(advice[0].kind, AdviceKind::WorkMem);
        assert_eq!(advice[0].sql.as_deref(), Some("SET work_mem = '128MB';"));
    }

    #[test]
    fn senala_el_filtro_que_el_indice_no_resolvio() {
        let advice = advise(&plan(
            r#"[{"Plan": {
                "Node Type": "Index Scan",
                "Relation Name": "public.trabajos",
                "Index Name": "trabajos_fecha_idx",
                "Index Cond": "(fecha > '2024-01-01'::date)",
                "Filter": "(estado = 'activo'::text)",
                "Rows Removed by Filter": 80000,
                "Actual Rows": 200,
                "Actual Total Time": 120.0,
                "Actual Loops": 1,
                "Plan Rows": 190,
                "Total Cost": 3000.0
            }}]"#,
        ));

        assert_eq!(advice.len(), 1);
        assert_eq!(advice[0].kind, AdviceKind::IndexFilter);
        assert_eq!(
            advice[0].sql.as_deref(),
            Some("CREATE INDEX ON public.trabajos (estado);")
        );
    }

    #[test]
    fn el_mismo_aviso_no_se_repite_por_cada_particion() {
        let hoja = r#"{
            "Node Type": "Seq Scan",
            "Relation Name": "ventas",
            "Filter": "(fecha > '2024-01-01'::date)",
            "Rows Removed by Filter": 50000,
            "Actual Rows": 100,
            "Actual Total Time": 40.0,
            "Actual Loops": 1,
            "Plan Rows": 100,
            "Total Cost": 900.0
        }"#;
        let json = format!(
            r#"[{{"Plan": {{"Node Type": "Append", "Plan Rows": 200, "Total Cost": 1800.0,
                 "Plans": [{hoja}, {hoja}]}}}}]"#
        );

        assert_eq!(advise(&plan(&json)).len(), 1);
    }
}

#[cfg(test)]
mod columnas {
    use super::filter_columns;

    #[test]
    fn saca_la_columna_de_una_igualdad() {
        assert_eq!(filter_columns("(estado = 'activo'::text)"), ["estado"]);
    }

    #[test]
    fn atraviesa_el_cast_y_los_parentesis_que_escribe_postgres() {
        assert_eq!(
            filter_columns("((estado)::text = 'activo'::text)"),
            ["estado"]
        );
    }

    #[test]
    fn se_queda_con_el_nombre_de_la_columna_y_no_con_el_alias() {
        assert_eq!(filter_columns("(t.estado = 'x'::text)"), ["estado"]);
    }

    #[test]
    fn pone_la_igualdad_antes_que_el_rango() {
        assert_eq!(
            filter_columns("((fecha >= '2024-01-01'::date) AND (estado = 'activo'::text))"),
            ["estado", "fecha"]
        );
    }

    #[test]
    fn ignora_lo_que_esta_adentro_de_una_funcion() {
        // `lower(nombre) = …` pide un índice por expresión; proponer uno por columna no serviría.
        assert!(filter_columns("(lower((nombre)::text) = 'ana'::text)").is_empty());
    }

    #[test]
    fn ignora_el_distinto_de_que_ningun_indice_resuelve() {
        assert!(filter_columns("(estado <> 'activo'::text)").is_empty());
    }

    #[test]
    fn no_toma_por_columna_lo_que_hay_adentro_de_una_cadena() {
        assert_eq!(filter_columns("(nota = 'x = y'::text)"), ["nota"]);
    }

    #[test]
    fn corta_en_tres_columnas() {
        assert_eq!(
            filter_columns("((a = 1) AND (b = 2) AND (c = 3) AND (d = 4))"),
            ["a", "b", "c"]
        );
    }
}
