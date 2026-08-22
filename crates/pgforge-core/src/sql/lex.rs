//! Lexer compartido de PostgreSQL: comillas, comentarios y delimitadores en un solo lugar.
//!
//! `split.rs` (partir un script en sentencias) y `format.rs` (darle forma) necesitan la misma
//! respuesta a la misma pregunta —¿este `;`, este `(` o esta palabra caen dentro de una cadena, un
//! comentario o un cuerpo `$$ … $$`?— y tenerla escrita dos veces es tenerla mal escrita en una de
//! las dos tarde o temprano. Acá vive una sola vez, como un recorrido que produce un flujo de
//! tokens en vez de una lista de sentencias.
//!
//! No es un tokenizador de SQL de verdad —no distingue palabra clave de identificador, ni separa
//! un número de una palabra pegada— porque ninguno de los dos consumidores lo necesita: partir un
//! script solo pregunta por `;` y por contenido, y formatear solo por palabras y paréntesis.

/// Qué es un tramo del texto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Uno o más espacios, tabs o saltos de línea seguidos.
    Whitespace,
    /// `-- hasta el fin de línea`, con el salto que la cierra si lo hay.
    LineComment,
    /// `/* … */`, con el anidado ya resuelto: un solo token cubre el bloque entero.
    BlockComment,
    /// `'…'` o `E'…'`, comillas incluidas.
    String,
    /// `"…"`, un identificador citado, comillas incluidas.
    QuotedIdent,
    /// `$tag$ … $tag$`, delimitadores incluidos.
    Dollar,
    /// Una corrida de caracteres de identificador: una palabra clave, un nombre sin citar, un
    /// número. `Word` y no «identificador»: acá no se sabe, ni hace falta, cuál de los tres es.
    Word,
    /// Cualquier otro carácter suelto: paréntesis, coma, `;`, operadores. De a uno, porque ni
    /// partir el script ni indentar necesitan agruparlos.
    Punct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    /// Dónde empieza, en caracteres y con base 0 — misma unidad que usa `split::Statement`.
    pub start: usize,
}

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

/// La corrida que se está acumulando en `Normal`: nunca los dos a la vez, así que alcanza con
/// saber cuál sigue abierta para decidir si hay que cerrarla antes de seguir.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Run {
    Whitespace,
    Word,
}

/// Recorre `sql` entero y lo parte en tokens. La concatenación de sus `text`, en orden, reconstruye
/// el texto de entrada exacto: no se descarta nada, ni siquiera los espacios.
pub fn lex(sql: &str) -> Vec<Token> {
    let chars: Vec<char> = sql.chars().collect();
    let mut tokens = Vec::new();
    let mut state = State::Normal;
    let mut i = 0;
    // Arranque del tramo que se está por cerrar: un comentario, una cadena, un identificador
    // citado, un bloque de dólares, o la corrida de espacio/palabra que se acumula en `Normal`.
    let mut piece_start = 0;
    let mut run: Option<Run> = None;

    while i < chars.len() {
        let c = chars[i];

        match &state {
            State::Normal => match c {
                '-' if chars.get(i + 1) == Some(&'-') => {
                    flush_run(&mut tokens, &chars, &mut run, piece_start, i);
                    piece_start = i;
                    state = State::LineComment;
                    i += 2;
                }
                '/' if chars.get(i + 1) == Some(&'*') => {
                    flush_run(&mut tokens, &chars, &mut run, piece_start, i);
                    piece_start = i;
                    state = State::BlockComment(1);
                    i += 2;
                }
                '\'' => {
                    flush_run(&mut tokens, &chars, &mut run, piece_start, i);
                    piece_start = i;
                    state = State::Quoted {
                        escape: is_escape_string(&chars, i),
                    };
                    i += 1;
                }
                '"' => {
                    flush_run(&mut tokens, &chars, &mut run, piece_start, i);
                    piece_start = i;
                    state = State::Ident;
                    i += 1;
                }
                '$' => match dollar_tag(&chars, i) {
                    Some(tag) => {
                        flush_run(&mut tokens, &chars, &mut run, piece_start, i);
                        piece_start = i;
                        i += tag.len();
                        state = State::Dollar(tag);
                    }
                    None => {
                        flush_run(&mut tokens, &chars, &mut run, piece_start, i);
                        tokens.push(Token {
                            kind: TokenKind::Punct,
                            text: "$".to_owned(),
                            start: i,
                        });
                        i += 1;
                    }
                },
                _ if c.is_whitespace() => {
                    if run != Some(Run::Whitespace) {
                        flush_run(&mut tokens, &chars, &mut run, piece_start, i);
                        piece_start = i;
                        run = Some(Run::Whitespace);
                    }
                    i += 1;
                }
                _ if is_ident_char(c) => {
                    if run != Some(Run::Word) {
                        flush_run(&mut tokens, &chars, &mut run, piece_start, i);
                        piece_start = i;
                        run = Some(Run::Word);
                    }
                    i += 1;
                }
                _ => {
                    flush_run(&mut tokens, &chars, &mut run, piece_start, i);
                    tokens.push(Token {
                        kind: TokenKind::Punct,
                        text: c.to_string(),
                        start: i,
                    });
                    i += 1;
                }
            },

            State::LineComment => {
                if c == '\n' {
                    i += 1;
                    tokens.push(span(TokenKind::LineComment, &chars, piece_start, i));
                    state = State::Normal;
                } else {
                    i += 1;
                }
            }

            State::BlockComment(depth) => {
                if c == '/' && chars.get(i + 1) == Some(&'*') {
                    state = State::BlockComment(depth + 1);
                    i += 2;
                } else if c == '*' && chars.get(i + 1) == Some(&'/') {
                    i += 2;
                    if *depth == 1 {
                        tokens.push(span(TokenKind::BlockComment, &chars, piece_start, i));
                        state = State::Normal;
                    } else {
                        state = State::BlockComment(depth - 1);
                    }
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
                        i += 1;
                        tokens.push(span(TokenKind::String, &chars, piece_start, i));
                        state = State::Normal;
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
                        i += 1;
                        tokens.push(span(TokenKind::QuotedIdent, &chars, piece_start, i));
                        state = State::Normal;
                    }
                } else {
                    i += 1;
                }
            }

            State::Dollar(tag) => {
                if chars[i..].starts_with(tag) {
                    i += tag.len();
                    tokens.push(span(TokenKind::Dollar, &chars, piece_start, i));
                    state = State::Normal;
                } else {
                    i += 1;
                }
            }
        }
    }

    // Lo que quedó abierto al llegar al final —una cadena sin cerrar, un comentario de bloque sin
    // `*/`— se entrega igual como un token más: acá no hay servidor al que rechazarle nada, y
    // perder esos caracteres rompería la promesa de que los tokens reconstruyen el texto entero.
    flush_run(&mut tokens, &chars, &mut run, piece_start, i);
    match state {
        State::Normal => {}
        State::LineComment => tokens.push(span(TokenKind::LineComment, &chars, piece_start, i)),
        State::BlockComment(_) => {
            tokens.push(span(TokenKind::BlockComment, &chars, piece_start, i))
        }
        State::Quoted { .. } => tokens.push(span(TokenKind::String, &chars, piece_start, i)),
        State::Ident => tokens.push(span(TokenKind::QuotedIdent, &chars, piece_start, i)),
        State::Dollar(_) => tokens.push(span(TokenKind::Dollar, &chars, piece_start, i)),
    }

    tokens
}

fn flush_run(
    tokens: &mut Vec<Token>,
    chars: &[char],
    run: &mut Option<Run>,
    piece_start: usize,
    i: usize,
) {
    if let Some(kind) = run.take() {
        let kind = match kind {
            Run::Whitespace => TokenKind::Whitespace,
            Run::Word => TokenKind::Word,
        };
        tokens.push(Token {
            kind,
            text: chars[piece_start..i].iter().collect(),
            start: piece_start,
        });
    }
}

fn span(kind: TokenKind, chars: &[char], start: usize, end: usize) -> Token {
    Token {
        kind,
        text: chars[start..end].iter().collect(),
        start,
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
