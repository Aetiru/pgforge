//! Partición de un script en sentencias.
//!
//! El editor manda texto libre: varias sentencias separadas por `;`, con comentarios y cuerpos de
//! función adentro. Partir por `;` a secas rompe cualquier `CREATE FUNCTION`, porque el cuerpo va
//! entre `$$ … $$` y tiene sus propios puntos y coma. Acá se recorre el flujo de tokens de
//! `super::lex` —que ya resolvió comillas, comentarios y delimitadores— y partir se reduce a
//! buscar los `;` que ese flujo dejó a nivel superior.
//!
//! Es una función pura a propósito: partir bien el script es la parte más fácil de arruinar y la
//! única que se puede verificar entera sin un servidor.

use serde::Serialize;

use super::lex::{lex, TokenKind};

/// Una sentencia dentro del script, ya recortada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Statement {
    /// El texto que se le manda al servidor, sin el `;` que la cierra.
    pub text: String,
    /// Dónde empieza `text` dentro del script, contado en caracteres y con base 0.
    ///
    /// En caracteres y no en bytes porque el único consumidor es la interfaz, y la posición de
    /// error que devuelve PostgreSQL también viene en caracteres: sumarlas exige que cuenten lo
    /// mismo.
    pub offset: usize,
    /// Línea donde empieza, con base 1.
    pub line: usize,
}

/// Parte el script en sentencias ejecutables.
///
/// Las que quedan vacías o solo con comentarios se descartan: mandarlas al servidor no haría nada
/// y desalinearía la numeración que la interfaz usa para señalar cuál falló.
pub fn split(sql: &str) -> Vec<Statement> {
    let chars: Vec<char> = sql.chars().collect();
    let mut statements = Vec::new();

    let mut start = 0;
    let mut has_content = false;

    for token in lex(sql) {
        // Un `;` que el lexer entregó como token suelto ya pasó el filtro de comillas, comentarios
        // y `$$ … $$`: cualquiera de esos casos lo hubiera dejado adentro de un token más grande.
        if token.kind == TokenKind::Punct && token.text == ";" {
            if has_content {
                statements.push(build(&chars, start, token.start));
            }
            start = token.start + 1;
            has_content = false;
            continue;
        }

        // Ni un comentario ni el espacio en blanco cuentan como contenido: una sentencia hecha
        // solo de eso no se manda al servidor.
        if !matches!(
            token.kind,
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment
        ) {
            has_content = true;
        }
    }

    // Lo último puede no llevar `;`: en un editor es el caso normal, no un error.
    if has_content {
        statements.push(build(&chars, start, chars.len()));
    }

    statements
}

/// La sentencia que contiene al cursor, para poder ejecutar solo esa.
///
/// Cuando el cursor cae en un hueco —una línea en blanco, un comentario suelto— manda la línea: si
/// está en la misma en la que termina la sentencia anterior, es esa; si bajó, es la siguiente. Es la
/// diferencia entre dejar el cursor después del `;` que uno acaba de escribir y haber bajado hasta
/// la sentencia que quiere ejecutar, y la regla anterior —siempre la anterior— hacía que pararse
/// arriba de la segunda sentencia ejecutara la primera.
pub fn at_cursor(sql: &str, cursor: usize) -> Option<Statement> {
    let statements = split(sql);

    if let Some(dentro) = statements.iter().find(|statement| {
        let end = statement.offset + statement.text.chars().count();
        cursor >= statement.offset && cursor <= end
    }) {
        return Some(dentro.clone());
    }

    let anterior = statements
        .iter()
        .rfind(|statement| statement.offset <= cursor);
    let siguiente = statements
        .iter()
        .find(|statement| statement.offset > cursor);

    match (anterior, siguiente) {
        (Some(previa), Some(proxima)) => {
            let fin = previa.offset + previa.text.chars().count();
            let bajo = sql.chars().skip(fin).take(cursor - fin).any(|c| c == '\n');
            Some(if bajo {
                proxima.clone()
            } else {
                previa.clone()
            })
        }
        (previa, proxima) => previa.or(proxima).cloned(),
    }
}

/// Palabras que arrancan una sentencia nueva de SQL, para [`missing_separator`].
const STATEMENT_STARTERS: &[&str] = &[
    "SELECT", "INSERT", "UPDATE", "DELETE", "WITH", "CREATE", "ALTER", "DROP", "TRUNCATE", "GRANT",
    "REVOKE", "COMMENT", "SET", "EXPLAIN", "VACUUM", "ANALYZE", "BEGIN", "COMMIT", "ROLLBACK",
];

/// Palabras que, inmediatamente antes de una de [`STATEMENT_STARTERS`], dicen que no es una
/// sentencia pegada sino la continuación de la misma: un operador de conjunto o el cuerpo de un
/// CTE o de una vista.
const CONTINUES_PREVIOUS: &[&str] = &["UNION", "INTERSECT", "EXCEPT", "AS"];

/// Dónde empieza la segunda sentencia de un texto que [`split`] entregó como una sola, si parece
/// que le falta el `;` que las separaba.
///
/// PostgreSQL no rechaza `SELECT now() SELECT …` en el segundo `SELECT`: un alias sin `AS` es un
/// `ColLabel` y acepta palabras reservadas, así que se lo traga como nombre de columna y el error
/// cae sobre el token siguiente — el usuario ve señalado algo que no tiene nada que ver.
///
/// Deliberadamente conservadora: esto solo agrega un aviso, y un aviso equivocado es peor que
/// ninguno. Solo mira palabras a profundidad de paréntesis 0 —una subconsulta nunca cuenta— y dos
/// formas de sentencia compuesta quedan afuera a propósito: un operador de conjunto
/// (`UNION`/`INTERSECT`/`EXCEPT`) y el `SELECT` que sigue al `AS` de un CTE o de una vista. La
/// sentencia principal de un `WITH` y el `SELECT` fuente de un `INSERT … SELECT` se exceptúan una
/// sola vez cada una: la segunda aparición sí cuenta.
pub fn missing_separator(sql: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut prev_word: Option<String> = None;
    let mut head: Option<String> = None;
    let mut with_main_pending = false;
    let mut insert_source_pending = false;

    for token in lex(sql) {
        match token.kind {
            TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment => continue,
            TokenKind::Punct => match token.text.as_str() {
                "(" => depth += 1,
                ")" => depth = depth.saturating_sub(1),
                _ => {}
            },
            TokenKind::Word if depth == 0 => {
                let upper = token.text.to_uppercase();

                if head.is_none() {
                    head = Some(upper.clone());
                    with_main_pending = upper == "WITH";
                    insert_source_pending = upper == "INSERT";
                } else if STATEMENT_STARTERS.contains(&upper.as_str()) {
                    let continues = prev_word
                        .as_deref()
                        .is_some_and(|w| CONTINUES_PREVIOUS.contains(&w));

                    if with_main_pending {
                        with_main_pending = false;
                    } else if upper == "SELECT" && insert_source_pending {
                        insert_source_pending = false;
                    } else if !continues {
                        return Some(token.start);
                    }
                }

                prev_word = Some(upper);
            }
            _ => {}
        }
    }

    None
}

/// Arma la sentencia recortando el espacio de los extremos y corrigiendo el desplazamiento.
///
/// El recorte importa: `offset` tiene que apuntar al primer carácter de lo que efectivamente se le
/// manda al servidor, o la posición del error caería desplazada en el editor.
fn build(chars: &[char], start: usize, end: usize) -> Statement {
    let mut from = start;
    let mut to = end;

    while from < to && chars[from].is_whitespace() {
        from += 1;
    }
    while to > from && chars[to - 1].is_whitespace() {
        to -= 1;
    }

    Statement {
        text: chars[from..to].iter().collect(),
        offset: from,
        line: 1 + chars[..from].iter().filter(|c| **c == '\n').count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn textos(sql: &str) -> Vec<String> {
        split(sql).into_iter().map(|s| s.text).collect()
    }

    #[test]
    fn separa_por_punto_y_coma() {
        assert_eq!(
            textos("SELECT 1; SELECT 2"),
            ["SELECT 1", "SELECT 2"],
            "la última no necesita cerrar con punto y coma"
        );
    }

    #[test]
    fn descarta_lo_vacio() {
        assert_eq!(textos(";;  ;\n"), Vec::<String>::new());
        assert_eq!(textos("SELECT 1;;"), ["SELECT 1"]);
    }

    #[test]
    fn descarta_lo_que_es_solo_comentario() {
        assert_eq!(textos("-- nada\n/* tampoco */"), Vec::<String>::new());
    }

    #[test]
    fn el_punto_y_coma_de_una_cadena_no_separa() {
        assert_eq!(textos("SELECT ';'"), ["SELECT ';'"]);
        assert_eq!(
            textos("SELECT 'a''b;c'; SELECT 2"),
            ["SELECT 'a''b;c'", "SELECT 2"],
            "la comilla duplicada es literal, no cierra la cadena"
        );
    }

    #[test]
    fn la_barra_solo_escapa_en_las_cadenas_e() {
        assert_eq!(
            textos(r"SELECT E'\';'; SELECT 2"),
            [r"SELECT E'\';'", "SELECT 2"]
        );
        // Sin el prefijo, la barra es un carácter más y la comilla cierra igual.
        assert_eq!(textos(r"SELECT '\'; SELECT 2"), [r"SELECT '\'", "SELECT 2"]);
    }

    #[test]
    fn el_punto_y_coma_de_un_identificador_citado_no_separa() {
        assert_eq!(
            textos(r#"SELECT * FROM "tabla;rara""#),
            [r#"SELECT * FROM "tabla;rara""#]
        );
    }

    #[test]
    fn el_cuerpo_entre_signos_pesos_queda_entero() {
        let sql = "CREATE FUNCTION f() RETURNS int LANGUAGE plpgsql AS $fn$
BEGIN
    PERFORM 1;
    RETURN 2;
END;
$fn$;
SELECT f()";

        let partes = textos(sql);
        assert_eq!(partes.len(), 2, "se partió el cuerpo de la función");
        assert!(partes[0].ends_with("$fn$"), "{}", partes[0]);
        assert!(partes[0].contains("RETURN 2;"), "{}", partes[0]);
        assert_eq!(partes[1], "SELECT f()");
    }

    #[test]
    fn el_delimitador_vacio_tambien_cuenta() {
        assert_eq!(textos("DO $$ BEGIN NULL; END $$; SELECT 1").len(), 2);
    }

    #[test]
    fn un_parametro_no_abre_un_bloque() {
        assert_eq!(
            textos("SELECT $1; SELECT $2"),
            ["SELECT $1", "SELECT $2"],
            "$1 no es un delimitador: si lo fuera, todo el resto quedaría adentro"
        );
    }

    #[test]
    fn los_comentarios_de_bloque_anidan() {
        assert_eq!(
            textos("/* uno /* dos */ sigue el comentario ; */ SELECT 1"),
            ["/* uno /* dos */ sigue el comentario ; */ SELECT 1"]
        );
    }

    #[test]
    fn el_punto_y_coma_de_un_comentario_de_linea_no_separa() {
        assert_eq!(textos("SELECT 1 -- ; no\n + 2"), ["SELECT 1 -- ; no\n + 2"]);
    }

    #[test]
    fn ubica_cada_sentencia_en_el_texto() {
        let sql = "SELECT 1;\n\n  SELECT 2;";
        let partes = split(sql);

        assert_eq!(partes[0].offset, 0);
        assert_eq!(partes[0].line, 1);
        assert_eq!(partes[1].line, 3);
        assert_eq!(
            &sql[partes[1].offset..partes[1].offset + partes[1].text.len()],
            "SELECT 2",
            "el desplazamiento debe caer sobre el primer carácter de la sentencia"
        );
    }

    #[test]
    fn el_desplazamiento_cuenta_caracteres_y_no_bytes() {
        let partes = split("SELECT 'ñandú'; SELECT 2");
        assert_eq!(
            partes[1].offset, 16,
            "16 caracteres, aunque sean 18 bytes en UTF-8"
        );
    }

    #[test]
    fn encuentra_la_sentencia_del_cursor() {
        let sql = "SELECT 1;\nSELECT 2;\nSELECT 3";

        assert_eq!(at_cursor(sql, 0).unwrap().text, "SELECT 1");
        assert_eq!(
            at_cursor(sql, 8).unwrap().text,
            "SELECT 1",
            "el final de la sentencia todavía es la sentencia"
        );
        assert_eq!(at_cursor(sql, 12).unwrap().text, "SELECT 2");
        assert_eq!(
            at_cursor(sql, sql.chars().count()).unwrap().text,
            "SELECT 3"
        );
    }

    #[test]
    fn en_un_hueco_manda_la_linea_del_cursor() {
        // `SELECT 1;` en la línea 1, en blanco la 2, `SELECT 2;` en la 3.
        let sql = "SELECT 1;\n\nSELECT 2;";

        assert_eq!(
            at_cursor(sql, 9).unwrap().text,
            "SELECT 1",
            "justo después del punto y coma sigue siendo la sentencia recién escrita"
        );
        assert_eq!(
            at_cursor(sql, 10).unwrap().text,
            "SELECT 2",
            "bajar a la línea en blanco de arriba de la segunda es querer ejecutar la segunda"
        );
    }

    #[test]
    fn un_comentario_entre_dos_sentencias_va_con_la_de_abajo() {
        // El comentario ya forma parte de la sentencia que lo sigue —se le manda al servidor con
        // ella, o la posición del error caería corrida—, así que el cursor adentro es esa.
        let sql = "SELECT 1;\n-- lo que sigue\nSELECT 2;";
        let comentario = sql.chars().position(|c| c == '-').unwrap();

        assert_eq!(
            at_cursor(sql, comentario + 3).unwrap().text,
            "-- lo que sigue\nSELECT 2"
        );
    }

    #[test]
    fn el_cursor_antes_de_la_primera_sentencia_la_elige_a_ella() {
        let sql = "\n\nSELECT 1;";
        assert_eq!(at_cursor(sql, 0).unwrap().text, "SELECT 1");
    }

    #[test]
    fn sin_sentencias_no_hay_nada_bajo_el_cursor() {
        assert!(at_cursor("-- vacío", 3).is_none());
    }

    #[test]
    fn delata_el_punto_y_coma_que_falta_entre_dos_sentencias() {
        let sql = "SELECT public.f(1, 2)\n\nSELECT array_agg(x) FROM (VALUES (1),(2)) t(x)";
        let at = missing_separator(sql).expect("debería detectar la sentencia pegada");
        assert_eq!(&sql[at..at + 6], "SELECT");
    }

    #[test]
    fn un_union_no_es_una_sentencia_pegada() {
        assert_eq!(missing_separator("SELECT 1 UNION SELECT 2"), None);
        assert_eq!(missing_separator("SELECT 1 INTERSECT SELECT 2"), None);
        assert_eq!(missing_separator("SELECT 1 EXCEPT SELECT 2"), None);
    }

    #[test]
    fn un_cte_no_es_una_sentencia_pegada() {
        assert_eq!(
            missing_separator("WITH x AS (SELECT 1) SELECT * FROM x"),
            None
        );
        assert_eq!(
            missing_separator("WITH x AS (SELECT 1), y AS (SELECT 2) SELECT * FROM x JOIN y"),
            None,
            "varios CTE seguidos no confunden cuál es la sentencia principal"
        );
    }

    #[test]
    fn una_vista_no_es_una_sentencia_pegada() {
        assert_eq!(missing_separator("CREATE VIEW v AS SELECT 1 FROM t"), None);
    }

    #[test]
    fn un_insert_select_no_es_una_sentencia_pegada() {
        assert_eq!(missing_separator("INSERT INTO t SELECT * FROM u"), None);
        assert_eq!(
            missing_separator("INSERT INTO t (a, b) SELECT x, y FROM u"),
            None
        );
    }

    #[test]
    fn un_select_adentro_de_un_parentesis_no_cuenta() {
        assert_eq!(
            missing_separator("SELECT * FROM t WHERE EXISTS (SELECT 1 FROM u)"),
            None
        );
    }

    #[test]
    fn un_select_adentro_de_una_cadena_o_de_un_cuerpo_no_cuenta() {
        assert_eq!(
            missing_separator("SELECT 'iría UNION SELECT si fuera código' FROM t"),
            None,
            "el SELECT de la cadena no es un token, así que no puede ser una sentencia pegada"
        );
        assert_eq!(
            missing_separator(
                "CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$\nBEGIN\n SELECT 1;\nEND\n$$"
            ),
            None,
            "el SELECT de adentro del cuerpo tampoco es un token propio"
        );
    }

    #[test]
    fn una_sentencia_sola_no_tiene_nada_pegado() {
        assert_eq!(missing_separator("SELECT 1"), None);
        assert_eq!(missing_separator(""), None);
    }
}
