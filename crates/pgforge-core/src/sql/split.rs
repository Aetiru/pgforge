//! Partición de un script en sentencias.
//!
//! El editor manda texto libre: varias sentencias separadas por `;`, con comentarios y cuerpos de
//! función adentro. Partir por `;` a secas rompe cualquier `CREATE FUNCTION`, porque el cuerpo va
//! entre `$$ … $$` y tiene sus propios puntos y coma. Acá se recorre el texto con las mismas reglas
//! léxicas que usa PostgreSQL para decidir cuándo un `;` separa de verdad.
//!
//! Es una función pura a propósito: partir bien el script es la parte más fácil de arruinar y la
//! única que se puede verificar entera sin un servidor.

use serde::Serialize;

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

/// Estado del recorrido. Todo lo que no es `Normal` es una región donde un `;` no separa nada.
enum State {
    Normal,
    /// `-- hasta el fin de línea`
    LineComment,
    /// `/* … */`, que en PostgreSQL anida.
    BlockComment(usize),
    /// `'…'`. `escape` distingue las cadenas `E'…'`, donde `\` escapa al carácter siguiente.
    Quoted {
        escape: bool,
    },
    /// `"…"`, un identificador citado.
    Ident,
    /// `$tag$ … $tag$`, con el delimitador completo incluidos los `$`.
    Dollar(Vec<char>),
}

/// Parte el script en sentencias ejecutables.
///
/// Las que quedan vacías o solo con comentarios se descartan: mandarlas al servidor no haría nada
/// y desalinearía la numeración que la interfaz usa para señalar cuál falló.
pub fn split(sql: &str) -> Vec<Statement> {
    let chars: Vec<char> = sql.chars().collect();
    let mut statements = Vec::new();

    let mut state = State::Normal;
    let mut start = 0;
    let mut has_content = false;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        match &state {
            State::Normal => match c {
                '-' if chars.get(i + 1) == Some(&'-') => {
                    state = State::LineComment;
                    i += 2;
                }
                '/' if chars.get(i + 1) == Some(&'*') => {
                    state = State::BlockComment(1);
                    i += 2;
                }
                '\'' => {
                    state = State::Quoted {
                        escape: is_escape_string(&chars, i),
                    };
                    has_content = true;
                    i += 1;
                }
                '"' => {
                    state = State::Ident;
                    has_content = true;
                    i += 1;
                }
                '$' => match dollar_tag(&chars, i) {
                    Some(tag) => {
                        i += tag.len();
                        state = State::Dollar(tag);
                        has_content = true;
                    }
                    None => {
                        has_content = true;
                        i += 1;
                    }
                },
                ';' => {
                    if has_content {
                        statements.push(build(&chars, start, i));
                    }
                    i += 1;
                    start = i;
                    has_content = false;
                }
                _ => {
                    if !c.is_whitespace() {
                        has_content = true;
                    }
                    i += 1;
                }
            },

            State::LineComment => {
                if c == '\n' {
                    state = State::Normal;
                }
                i += 1;
            }

            State::BlockComment(depth) => {
                if c == '/' && chars.get(i + 1) == Some(&'*') {
                    state = State::BlockComment(depth + 1);
                    i += 2;
                } else if c == '*' && chars.get(i + 1) == Some(&'/') {
                    state = match depth {
                        1 => State::Normal,
                        _ => State::BlockComment(depth - 1),
                    };
                    i += 2;
                } else {
                    i += 1;
                }
            }

            State::Quoted { escape } => {
                if *escape && c == '\\' {
                    // El carácter escapado se salta entero: `E'\''` no cierra la cadena.
                    i += 2;
                } else if c == '\'' {
                    // Dos comillas seguidas son una comilla literal, no el cierre.
                    if chars.get(i + 1) == Some(&'\'') {
                        i += 2;
                    } else {
                        state = State::Normal;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }

            State::Ident => {
                if c == '"' {
                    if chars.get(i + 1) == Some(&'"') {
                        i += 2;
                    } else {
                        state = State::Normal;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }

            State::Dollar(tag) => {
                if chars[i..].starts_with(tag) {
                    i += tag.len();
                    state = State::Normal;
                } else {
                    i += 1;
                }
            }
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

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `true` si la comilla en `quote` abre una cadena `E'…'`, donde `\` escapa.
///
/// Se mira que la `E` no sea el final de un identificador: en `caste'x'` la `e` es parte del
/// nombre, no un prefijo de cadena.
fn is_escape_string(chars: &[char], quote: usize) -> bool {
    if quote == 0 || !matches!(chars[quote - 1], 'e' | 'E') {
        return false;
    }
    quote < 2 || !is_ident_char(chars[quote - 2])
}

/// Devuelve el delimitador completo (`$$`, `$cuerpo$`) si en `at` empieza uno.
///
/// `None` cuando el `$` es otra cosa: el parámetro `$1`, o el operador de un tipo definido por una
/// extensión.
fn dollar_tag(chars: &[char], at: usize) -> Option<Vec<char>> {
    let mut end = at + 1;

    while end < chars.len() && chars[end] != '$' {
        // La etiqueta sigue las reglas de un identificador, así que `$1` no abre nada.
        if !is_ident_char(chars[end]) || (end == at + 1 && chars[end].is_numeric()) {
            return None;
        }
        end += 1;
    }

    (end < chars.len()).then(|| chars[at..=end].to_vec())
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
}
