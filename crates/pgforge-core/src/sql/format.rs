//! El formateador de SQL.
//!
//! No analiza la gramática de PostgreSQL —eso son meses de trabajo—: recorre el flujo de tokens de
//! `super::lex` con dos cosas nada más, una tabla de palabras clave y la profundidad de paréntesis.
//! Un paréntesis que abre después de una palabra de cláusula (una subconsulta, el cuerpo de un CTE,
//! las columnas de un `CREATE TABLE`) suma un nivel de sangría; el que abre después de un nombre
//! (una llamada a función, `OVER (…)`, `ARRAY[…]`, una tupla de `VALUES`) no rompe línea. Esa sola
//! distinción cubre casi todo sin gramática.
//!
//! Reglas que no se negocian: los identificadores no cambian de caso nunca —solo las palabras
//! clave van a mayúscula—, los cuerpos `$$ … $$` se emiten tal cual, y los comentarios se
//! conservan siempre. Lo que sostiene la promesa de no romper nada es la guarda de equivalencia
//! (`equivalent`): antes de devolver, se vuelve a lexar la propia salida y se la compara contra la
//! entrada tolerando solo espacios y el caso de las palabras clave. Si difieren en cualquier otra
//! cosa, se devuelve la entrada sin tocar — el peor caso es «no formateó», nunca «formateó mal».

use serde::{Deserialize, Serialize};

use super::lex::{lex, Token, TokenKind};
use super::split::split;

/// Cómo se escriben las palabras clave. Los identificadores **nunca** cambian de caso: esto solo
/// decide `SELECT` / `select` / lo que el usuario haya escrito.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeywordCase {
    #[default]
    Upper,
    Lower,
    /// Cada palabra clave queda como se tipeó. Una frase de dos palabras (`GROUP BY`, `LEFT JOIN`)
    /// se emite con cada palabra tal cual y un solo espacio entre ellas.
    Preserve,
}

/// Con qué se sangra cada nivel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Indent {
    #[serde(rename = "2")]
    Two,
    #[default]
    #[serde(rename = "4")]
    Four,
    #[serde(rename = "tab")]
    Tab,
}

impl Indent {
    fn unit(self) -> &'static str {
        match self {
            Indent::Two => "  ",
            Indent::Four => "    ",
            Indent::Tab => "\t",
        }
    }
}

/// Las preferencias de estilo. Viajan del editor a cada llamada: el formateador sigue siendo puro,
/// sin leer ninguna configuración de afuera. `#[serde(default)]` para que una preferencia guardada
/// por una versión anterior —con menos campos— siga abriendo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FormatOptions {
    pub keyword_case: KeywordCase,
    pub indent: Indent,
    /// Líneas en blanco entre sentencias (0 a 3).
    pub lines_between_queries: u8,
    /// La coma al principio de cada línea (`, columna`) en vez de al final.
    pub comma_first: bool,
}

impl Default for FormatOptions {
    /// Lo que el formateador hacía antes de tener opciones: `format()` no puede cambiar su salida.
    fn default() -> Self {
        Self {
            keyword_case: KeywordCase::Upper,
            indent: Indent::Four,
            lines_between_queries: 1,
            comma_first: false,
        }
    }
}

/// Formatea con el estilo de fábrica.
pub fn format(sql: &str) -> String {
    format_with(sql, &FormatOptions::default())
}

/// Formatea un script entero, sentencia por sentencia, separándolas con `;` y líneas en blanco.
///
/// Pura: el mismo texto de entrada y las mismas opciones dan siempre el mismo resultado, sin tocar
/// el servidor.
pub fn format_with(sql: &str, options: &FormatOptions) -> String {
    let statements = split(sql);
    if statements.is_empty() {
        return sql.to_owned();
    }

    let trailing_semicolon = ends_with_semicolon(sql);

    let mut out = String::new();
    let last = statements.len() - 1;
    for (index, statement) in statements.iter().enumerate() {
        if index > 0 {
            // Un tope de 3 líneas en blanco: más que eso es un error de tipeo de quien configura,
            // y el valor llega de la interfaz.
            out.push(';');
            out.push_str(&"\n".repeat(usize::from(options.lines_between_queries.min(3)) + 1));
        }
        out.push_str(&format_statement(&statement.text, options));
        if index == last && trailing_semicolon {
            out.push(';');
        }
    }

    // La guarda es lo que sostiene soltar esto sin cubrir toda la gramática de SQL: si el recorrido
    // de arriba perdió o corrompió algo, acá se nota y se devuelve la entrada intacta.
    if equivalent(sql, &out) {
        out
    } else {
        sql.to_owned()
    }
}

/// Si lo último con contenido del script es un `;` de nivel superior.
///
/// `split()` no lo dice directamente: sus sentencias vienen sin el `;` que las cierra, y la última
/// puede o no haber tenido uno en el texto original. Sin esto, formatear `SELECT 1` —sin punto y
/// coma, el caso más común al escribir en el editor— le agregaría uno que la entrada no tenía, y
/// la guarda de equivalencia lo rechazaría siempre.
fn ends_with_semicolon(sql: &str) -> bool {
    lex(sql)
        .into_iter()
        .rev()
        .find(|token| {
            !matches!(
                token.kind,
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment
            )
        })
        .is_some_and(|token| token.kind == TokenKind::Punct && token.text == ";")
}

/// Compara dos scripts token a token, ignorando espacios y tolerando que una palabra cambie de
/// mayúscula a minúscula (o al revés): es la única diferencia que el formateador se permite.
fn equivalent(original: &str, candidate: &str) -> bool {
    let a = significant(original);
    let b = significant(candidate);
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| same_token(x, y))
}

fn significant(sql: &str) -> Vec<Token> {
    lex(sql)
        .into_iter()
        .filter(|token| token.kind != TokenKind::Whitespace)
        .collect()
}

fn same_token(a: &Token, b: &Token) -> bool {
    if a.kind != b.kind {
        return false;
    }
    match a.kind {
        // Una palabra puede ser una clave que se pasó a mayúscula, o un identificador que no se
        // tocó: en los dos casos, mayúsculas de los dos lados tienen que coincidir igual.
        TokenKind::Word => a.text.to_uppercase() == b.text.to_uppercase(),
        // Cadenas, identificadores citados, cuerpos `$$…$$`, comentarios y puntuación: si el
        // formateador los tocó, es un error y la guarda tiene que verlo — comparación literal.
        _ => a.text == b.text,
    }
}

// ---------------------------------------------------------------------------
// Clasificación de palabras
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Rompe línea y vuelve al margen del nivel: `SELECT`, `FROM`, `WHERE`, `GROUP BY`, …
    Clause,
    /// `JOIN` y sus variantes: rompe línea al margen; el `ON` que sigue queda en la misma.
    Join,
    /// `AND` / `OR`: rompe línea con 2 espacios más que su cláusula.
    SecondLevel,
    Case,
    When,
    Then,
    Else,
    End,
    /// Cualquier otra palabra clave: se pasa a mayúscula pero no rompe línea.
    Keyword,
    /// No es palabra clave: identificador, alias, número — se conserva tal cual llegó.
    Plain,
}

/// Los dos de que se arman con dos palabras seguidas: sin esto, `GROUP` por un lado y `BY` por el
/// otro no dirían nada del renglón que tienen que abrir juntas.
const TWO_WORD_CLAUSES: &[&str] = &["GROUP BY", "ORDER BY", "INSERT INTO", "DELETE FROM"];

/// Modificadores que preceden a `JOIN`: la cadena entera (`LEFT OUTER JOIN`) es una sola unidad.
const JOIN_MODIFIERS: &[&str] = &[
    "LEFT", "RIGHT", "INNER", "CROSS", "FULL", "OUTER", "NATURAL", "LATERAL",
];

fn role_of(upper: &str) -> Role {
    match upper {
        "SELECT" | "FROM" | "WHERE" | "HAVING" | "LIMIT" | "OFFSET" | "VALUES" | "SET"
        | "RETURNING" | "WITH" | "UNION" | "INTERSECT" | "EXCEPT" | "UPDATE" => Role::Clause,
        "AND" | "OR" => Role::SecondLevel,
        "CASE" => Role::Case,
        "WHEN" => Role::When,
        "THEN" => Role::Then,
        "ELSE" => Role::Else,
        "END" => Role::End,
        "AS" | "ON" | "USING" | "IN" | "NOT" | "NULL" | "IS" | "LIKE" | "ILIKE" | "BETWEEN"
        | "DISTINCT" | "ASC" | "DESC" | "ALL" | "ANY" | "SOME" | "EXISTS" | "NULLS" | "FIRST"
        | "LAST" | "INTO" | "DEFAULT" | "TRUE" | "FALSE" | "UNKNOWN" | "OVER" | "PARTITION"
        | "FILTER" | "WINDOW" | "CONFLICT" | "DO" | "NOTHING" | "CAST" | "COLLATE" | "ARRAY"
        | "BY" | "ONLY" | "CREATE" | "ALTER" | "DROP" | "TABLE" | "VIEW" | "INDEX" | "TRIGGER"
        | "FUNCTION" | "PROCEDURE" | "SCHEMA" | "DATABASE" | "EXTENSION" | "DOMAIN" | "TYPE"
        | "SEQUENCE" | "ROLE" | "POLICY" | "GRANT" | "REVOKE" | "TO" | "UNIQUE" | "CHECK"
        | "CONSTRAINT" | "PRIMARY" | "FOREIGN" | "KEY" | "REFERENCES" | "CASCADE" | "RESTRICT"
        | "RETURNS" | "LANGUAGE" | "REPLACE" | "TEMP" | "TEMPORARY" | "IF" | "EXECUTE"
        | "BEGIN" | "COMMIT" | "ROLLBACK" => Role::Keyword,
        _ => Role::Plain,
    }
}

// ---------------------------------------------------------------------------
// Unidades: palabras ya agrupadas, puntuación y bloques que se emiten tal cual
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Unit {
    /// Una palabra, ya con el caso que le corresponde y su rol resuelto.
    Word { display: String, role: Role },
    /// Un carácter suelto: paréntesis, coma, `;`, operadores.
    Punct(char),
    /// Cadena, identificador citado, cuerpo `$$…$$` o comentario: se emite tal cual, sin tocar.
    Atomic(String),
}

fn units_of(tokens: &[Token], case: KeywordCase) -> Vec<Unit> {
    // Los espacios los recalcula el formateador entero; no aportan nada acá.
    let words: Vec<&Token> = tokens
        .iter()
        .filter(|token| token.kind != TokenKind::Whitespace)
        .collect();

    let mut units = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let token = words[i];
        match token.kind {
            TokenKind::Word => {
                let upper = token.text.to_uppercase();

                if let Some(next) = words.get(i + 1).filter(|t| t.kind == TokenKind::Word) {
                    let pair = format!("{upper} {}", next.text.to_uppercase());
                    if TWO_WORD_CLAUSES.contains(&pair.as_str()) {
                        units.push(Unit::Word {
                            display: cased_phrase(&[token, next], case),
                            role: Role::Clause,
                        });
                        i += 2;
                        continue;
                    }
                }

                if upper == "JOIN" || JOIN_MODIFIERS.contains(&upper.as_str()) {
                    if let Some(end) = join_end(&words, i) {
                        units.push(Unit::Word {
                            display: cased_phrase(&words[i..=end], case),
                            role: Role::Join,
                        });
                        i = end + 1;
                        continue;
                    }
                }

                let role = role_of(&upper);
                let display = if role == Role::Plain {
                    token.text.clone()
                } else {
                    cased(&token.text, case)
                };
                units.push(Unit::Word { display, role });
                i += 1;
            }
            TokenKind::Punct => {
                units.push(Unit::Punct(token.text.chars().next().unwrap_or(' ')));
                i += 1;
            }
            _ => {
                units.push(Unit::Atomic(token.text.clone()));
                i += 1;
            }
        }
    }
    units
}

fn cased(word: &str, case: KeywordCase) -> String {
    match case {
        KeywordCase::Upper => word.to_uppercase(),
        KeywordCase::Lower => word.to_lowercase(),
        KeywordCase::Preserve => word.to_owned(),
    }
}

/// Una frase de varias palabras clave (`GROUP BY`, `LEFT OUTER JOIN`) con un solo espacio entre
/// ellas, cada una con el caso pedido.
fn cased_phrase(words: &[&Token], case: KeywordCase) -> String {
    words
        .iter()
        .map(|word| cased(&word.text, case))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Busca el `JOIN` que cierra una cadena de modificadores (`LEFT OUTER JOIN`) empezando en `start`.
/// `None` si algo corta la cadena antes de llegar a `JOIN` — no es una unión, es otra cosa.
fn join_end(words: &[&Token], start: usize) -> Option<usize> {
    let mut i = start;
    loop {
        let token = words.get(i)?;
        if token.kind != TokenKind::Word {
            return None;
        }
        let upper = token.text.to_uppercase();
        if upper == "JOIN" {
            return Some(i);
        }
        if !JOIN_MODIFIERS.contains(&upper.as_str()) {
            return None;
        }
        i += 1;
    }
}

/// Si un paréntesis que sigue a `prev` abre un bloque (subconsulta, CTE, columnas de una tabla) o
/// se queda inline (llamada a función, `OVER (…)`, `ARRAY[…]`, la tupla de un `VALUES`).
fn opens_inline(prev: Option<&Unit>) -> bool {
    match prev {
        None => false,
        Some(Unit::Word { display, role }) => {
            matches!(role, Role::Keyword | Role::Plain) || display.eq_ignore_ascii_case("VALUES")
        }
        Some(Unit::Punct(')')) => true,
        _ => false,
    }
}

/// Si hay al menos un paréntesis que realmente abre bloque (no uno inline ni uno adentro de otro
/// inline): eso solo es una sentencia «corta» si además tiene una sola cláusula.
fn has_breaking_paren(units: &[Unit]) -> bool {
    let mut suppressed = 0usize;
    let mut stack: Vec<bool> = Vec::new();

    for (index, unit) in units.iter().enumerate() {
        match unit {
            Unit::Punct('(') => {
                let prev = if index == 0 {
                    None
                } else {
                    Some(&units[index - 1])
                };
                let inline = suppressed > 0 || opens_inline(prev);
                if !inline {
                    return true;
                }
                suppressed += 1;
                stack.push(true);
            }
            Unit::Punct(')') => {
                if let Some(true) = stack.pop() {
                    suppressed = suppressed.saturating_sub(1);
                }
            }
            _ => {}
        }
    }
    false
}

/// Si la sentencia necesita partirse en líneas, o si es corta y se deja en una sola.
fn needs_breaking(units: &[Unit]) -> bool {
    let clause_like = units
        .iter()
        .filter(|unit| {
            matches!(
                unit,
                Unit::Word {
                    role: Role::Clause | Role::Join,
                    ..
                }
            )
        })
        .count();
    let has_second_level = units.iter().any(|unit| {
        matches!(
            unit,
            Unit::Word {
                role: Role::SecondLevel,
                ..
            }
        )
    });
    let has_case = units.iter().any(|unit| {
        matches!(
            unit,
            Unit::Word {
                role: Role::Case,
                ..
            }
        )
    });

    clause_like >= 2 || has_second_level || has_case || has_breaking_paren(units)
}

// ---------------------------------------------------------------------------
// Impresión
// ---------------------------------------------------------------------------

/// Un contexto donde una coma de nivel superior fuerza salto de línea al margen indicado.
enum Block {
    /// Paréntesis que no rompe línea: llamada a función, `OVER(…)`, `ARRAY[…]`, tupla de `VALUES`.
    Inline,
    /// Un paréntesis que sí abre bloque, o la lista de columnas del `SELECT`.
    List { indent: usize, is_select: bool },
}

struct Printer {
    out: String,
    /// Nivel de sangría vigente, en cantidad de niveles.
    level: usize,
    blocks: Vec<Block>,
    /// Nivel al que hay que volver con el `END` de cada `CASE` abierto.
    case_stack: Vec<usize>,
    /// Cuántos paréntesis inline hay abiertos: mientras sea mayor que cero, nada rompe línea —es
    /// lo que hace que un `ORDER BY` adentro de un `OVER (…)` no fuerce un salto que la llamada
    /// entera tiene que evitar.
    suppressed: usize,
    fresh_line: bool,
    /// Si lo último que se escribió no admite espacio después (`(`, `[`, `.`, `:`).
    prev_tight_after: bool,
    /// Si lo último que se escribió fue un carácter suelto (paréntesis, operador). Dos puntos
    /// sueltos seguidos nunca llevan espacio entre sí, vengan o no de la misma tabla: separarlos
    /// partiría un operador de dos caracteres (`::`, `@>`, `<=`) en dos que ya no dicen lo mismo,
    /// y esa clase de error el lexer no lo puede notar —para él son dos símbolos sueltos, antes y
    /// después—.
    prev_was_punct: bool,
    /// Corto y sin nada que lo obligue a partirse: se imprime todo en una sola línea.
    oneline: bool,
    /// Lo que se repite por cada nivel de sangría.
    indent: &'static str,
    comma_first: bool,
}

impl Printer {
    fn new(oneline: bool, options: &FormatOptions) -> Self {
        Printer {
            indent: options.indent.unit(),
            comma_first: options.comma_first,
            out: String::new(),
            level: 0,
            blocks: Vec::new(),
            case_stack: Vec::new(),
            suppressed: 0,
            fresh_line: true,
            prev_tight_after: false,
            prev_was_punct: false,
            oneline,
        }
    }

    fn finish(self) -> String {
        self.out
    }

    /// Escribe `text` respetando el espacio que le toca: ninguno si la línea recién empezó, si
    /// `tight_before` lo pide (una coma, un paréntesis que cierra), si lo anterior no admite nada
    /// después (`(`, `.`) o si los dos lados son símbolos sueltos; uno en cualquier otro caso.
    fn raw(&mut self, text: &str, tight_before: bool, tight_after: bool, is_punct: bool) {
        let glued_puncts = is_punct && self.prev_was_punct;
        if !self.fresh_line && !tight_before && !glued_puncts && !self.prev_tight_after {
            self.out.push(' ');
        }
        self.out.push_str(text);
        self.fresh_line = false;
        self.prev_tight_after = tight_after;
        self.prev_was_punct = is_punct;
    }

    fn newline_to(&mut self, level: usize) {
        self.out.push('\n');
        self.out.push_str(&self.indent.repeat(level));
        self.fresh_line = true;
    }

    fn emit(&mut self, unit: &Unit) {
        match unit {
            Unit::Word { display, .. } => self.raw(display, false, false, false),
            Unit::Punct(c) => {
                let tight_before = matches!(c, ',' | ')' | ']' | '.' | ';' | ':');
                let tight_after = matches!(c, '(' | '[' | '.' | ':');
                self.raw(&c.to_string(), tight_before, tight_after, true);
            }
            Unit::Atomic(text) => {
                self.raw(text, false, false, false);
                // Un comentario de línea ya cerró con su propio salto: no hace falta que el
                // próximo token empiece con un espacio de más.
                if text.ends_with('\n') {
                    self.fresh_line = true;
                }
            }
        }
    }

    fn open_paren(&mut self, prev: Option<&Unit>) {
        let inline = self.suppressed > 0 || opens_inline(prev);
        // Pegado al nombre en una llamada a función; con un espacio antes, como cualquier otra
        // palabra, cuando abre un bloque.
        self.raw("(", inline, true, true);
        if inline {
            self.blocks.push(Block::Inline);
            self.suppressed += 1;
        } else {
            self.level += 1;
            self.blocks.push(Block::List {
                indent: self.level,
                is_select: false,
            });
            self.newline_to(self.level);
        }
    }

    fn close_paren(&mut self) {
        match self.blocks.pop() {
            Some(Block::Inline) => {
                self.suppressed = self.suppressed.saturating_sub(1);
                self.raw(")", true, false, true);
            }
            Some(Block::List { .. }) => {
                self.level = self.level.saturating_sub(1);
                self.newline_to(self.level);
                self.raw(")", true, false, true);
            }
            // Un paréntesis de más que el lexer no encontró abierto en ningún lado no debería
            // pasar con SQL válido, pero no hay por qué entrar en pánico por eso.
            None => self.raw(")", true, false, true),
        }
    }

    fn end_select_list_if_open(&mut self) {
        if matches!(
            self.blocks.last(),
            Some(Block::List {
                is_select: true,
                ..
            })
        ) {
            self.blocks.pop();
        }
    }

    fn clause(&mut self, display: &str) {
        self.end_select_list_if_open();
        // Sin esto, la primera cláusula de la sentencia —o la que sigue justo a un paréntesis que
        // acaba de abrir bloque— se encontraría ya en una línea recién empezada y agregaría una de
        // más antes de escribirse.
        if !self.fresh_line {
            self.newline_to(self.level);
        }
        self.raw(display, false, false, false);
    }

    fn begin_select_list(&mut self) {
        let indent = self.level + 1;
        self.blocks.push(Block::List {
            indent,
            is_select: true,
        });
        self.newline_to(indent);
    }

    fn list_comma(&mut self) {
        match self.blocks.last() {
            Some(Block::List { indent, .. }) => {
                let indent = *indent;
                if self.comma_first {
                    self.comma_first_break(indent);
                } else {
                    self.raw(",", true, false, true);
                    self.newline_to(indent);
                }
            }
            _ => self.raw(",", true, false, true),
        }
    }

    /// La coma va al principio de la línea siguiente, dos columnas a la izquierda del elemento para
    /// que este quede alineado con el primero de la lista (`  , columna`). Con tabuladores no hay
    /// «dos columnas» que restar: la coma ocupa el lugar del último tabulador.
    fn comma_first_break(&mut self, level: usize) {
        self.out.push('\n');
        match self.indent {
            "\t" => self.out.push_str(&"\t".repeat(level.saturating_sub(1))),
            unit => {
                let width = (unit.len() * level).saturating_sub(2);
                self.out.push_str(&" ".repeat(width));
            }
        }
        self.out.push_str(", ");
        // El próximo elemento sigue pegado a la coma: el espacio ya está puesto.
        self.fresh_line = true;
        self.prev_tight_after = false;
        self.prev_was_punct = false;
    }

    fn second_level(&mut self, display: &str) {
        self.newline_to(self.level);
        // Dos espacios más que el margen de la cláusula: `newline_to` ya puso el margen, así que
        // alcanza con agregarlos antes de la palabra.
        self.out.push_str("  ");
        self.raw(display, true, false, false);
    }

    fn open_case(&mut self, display: &str) {
        self.end_select_list_if_open();
        self.case_stack.push(self.level);
        self.raw(display, false, false, false);
    }

    fn case_line(&mut self, display: &str) {
        let base = *self.case_stack.last().unwrap_or(&self.level);
        self.newline_to(base + 1);
        self.raw(display, true, false, false);
    }

    fn close_case(&mut self, display: &str) {
        let base = self.case_stack.pop().unwrap_or(self.level);
        self.newline_to(base);
        self.raw(display, true, false, false);
    }

    fn run(&mut self, units: &[Unit]) {
        for index in 0..units.len() {
            let unit = &units[index];
            let prev = if index == 0 {
                None
            } else {
                Some(&units[index - 1])
            };

            if self.oneline {
                match unit {
                    Unit::Punct('(') => self.open_paren(prev),
                    Unit::Punct(')') => self.close_paren(),
                    _ => self.emit(unit),
                }
                continue;
            }

            match unit {
                Unit::Punct('(') => self.open_paren(prev),
                Unit::Punct(')') => self.close_paren(),
                Unit::Punct(',') if self.suppressed == 0 && self.in_list() => self.list_comma(),
                Unit::Word {
                    role: Role::Clause,
                    display,
                } if self.suppressed == 0 => {
                    self.clause(display);
                    if display.eq_ignore_ascii_case("SELECT") {
                        self.begin_select_list();
                    }
                }
                Unit::Word {
                    role: Role::Join,
                    display,
                } if self.suppressed == 0 => {
                    self.clause(display);
                }
                Unit::Word {
                    role: Role::SecondLevel,
                    display,
                } if self.suppressed == 0 => {
                    self.second_level(display);
                }
                Unit::Word {
                    role: Role::Case,
                    display,
                } if self.suppressed == 0 => {
                    self.open_case(display);
                }
                Unit::Word {
                    role: Role::When,
                    display,
                } if self.suppressed == 0 => {
                    self.case_line(display);
                }
                Unit::Word {
                    role: Role::Else,
                    display,
                } if self.suppressed == 0 => {
                    self.case_line(display);
                }
                Unit::Word {
                    role: Role::End,
                    display,
                } if self.suppressed == 0 => {
                    self.close_case(display);
                }
                _ => self.emit(unit),
            }
        }
    }

    fn in_list(&self) -> bool {
        matches!(self.blocks.last(), Some(Block::List { .. }))
    }
}

fn format_statement(text: &str, options: &FormatOptions) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let tokens = lex(trimmed);
    let units = units_of(&tokens, options.keyword_case);
    if units.is_empty() {
        return trimmed.to_owned();
    }

    let mut printer = Printer::new(!needs_breaking(&units), options);
    printer.run(&units);
    printer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn con(sql: &str, options: FormatOptions) -> String {
        format_with(sql, &options)
    }

    #[test]
    fn las_opciones_de_fabrica_no_cambian_la_salida() {
        let sql = "select a, b from t where x = 1; select 2";
        assert_eq!(format(sql), format_with(sql, &FormatOptions::default()));
    }

    #[test]
    fn palabras_clave_en_minuscula() {
        let out = con(
            "SELECT a, b FROM t LEFT JOIN u ON u.id = t.id GROUP BY a",
            FormatOptions {
                keyword_case: KeywordCase::Lower,
                ..Default::default()
            },
        );
        assert!(out.starts_with("select\n"), "{out}");
        assert!(out.contains("\nfrom t"), "{out}");
        assert!(out.contains("\nleft join u on"), "{out}");
        assert!(out.contains("\ngroup by a"), "{out}");
    }

    #[test]
    fn preservar_deja_cada_palabra_como_se_escribio() {
        let out = con(
            "Select a, b From t Group By a",
            FormatOptions {
                keyword_case: KeywordCase::Preserve,
                ..Default::default()
            },
        );
        assert!(out.starts_with("Select\n"), "{out}");
        assert!(out.contains("\nFrom t"), "{out}");
        assert!(out.contains("\nGroup By a"), "{out}");
    }

    #[test]
    fn los_identificadores_siguen_igual_con_cualquier_caso() {
        let out = con(
            "SELECT MiColumna FROM MiTabla",
            FormatOptions {
                keyword_case: KeywordCase::Lower,
                ..Default::default()
            },
        );
        assert!(
            out.contains("MiColumna") && out.contains("MiTabla"),
            "{out}"
        );
    }

    #[test]
    fn sangria_de_dos_espacios_y_de_tabulador() {
        let sql = "select a, b from t";
        let dos = con(
            sql,
            FormatOptions {
                indent: Indent::Two,
                ..Default::default()
            },
        );
        assert!(dos.contains("\n  a,\n  b"), "{dos}");
        let tab = con(
            sql,
            FormatOptions {
                indent: Indent::Tab,
                ..Default::default()
            },
        );
        assert!(tab.contains("\n\ta,\n\tb"), "{tab}");
    }

    #[test]
    fn lineas_en_blanco_entre_sentencias() {
        let sql = "select 1; select 2";
        let ninguna = con(
            sql,
            FormatOptions {
                lines_between_queries: 0,
                ..Default::default()
            },
        );
        assert_eq!(ninguna, "SELECT 1;\nSELECT 2");
        let dos = con(
            sql,
            FormatOptions {
                lines_between_queries: 2,
                ..Default::default()
            },
        );
        assert_eq!(dos, "SELECT 1;\n\n\nSELECT 2");
        let tope = con(
            sql,
            FormatOptions {
                lines_between_queries: 200,
                ..Default::default()
            },
        );
        assert_eq!(tope, "SELECT 1;\n\n\n\nSELECT 2");
    }

    #[test]
    fn coma_al_principio() {
        let out = con(
            "select a, b, c from t",
            FormatOptions {
                comma_first: true,
                ..Default::default()
            },
        );
        assert_eq!(out, "SELECT\n    a\n  , b\n  , c\nFROM t");
    }

    #[test]
    fn coma_al_principio_con_tabulador_y_con_dos_espacios() {
        let tab = con(
            "select a, b from t",
            FormatOptions {
                comma_first: true,
                indent: Indent::Tab,
                ..Default::default()
            },
        );
        assert_eq!(tab, "SELECT\n\ta\n, b\nFROM t");
        let dos = con(
            "select a, b from t",
            FormatOptions {
                comma_first: true,
                indent: Indent::Two,
                ..Default::default()
            },
        );
        assert_eq!(dos, "SELECT\n  a\n, b\nFROM t");
    }

    #[test]
    fn las_opciones_viajan_en_camel_case() {
        let json =
            r#"{"keywordCase":"lower","indent":"tab","linesBetweenQueries":2,"commaFirst":true}"#;
        let options: FormatOptions = serde_json::from_str(json).unwrap();
        assert_eq!(options.keyword_case, KeywordCase::Lower);
        assert_eq!(options.indent, Indent::Tab);
        assert_eq!(options.lines_between_queries, 2);
        assert!(options.comma_first);
        // Una preferencia vieja, con menos campos, sigue abriendo con lo de fábrica.
        let viejo: FormatOptions = serde_json::from_str(r#"{"indent":"2"}"#).unwrap();
        assert_eq!(viejo.indent, Indent::Two);
        assert_eq!(viejo.keyword_case, KeywordCase::Upper);
    }

    #[test]
    fn pone_cada_columna_del_select_en_su_linea() {
        let out = format("select c.id, c.nombre, sum(p.total) as total from clientes c");
        assert_eq!(
            out,
            "SELECT\n    c.id,\n    c.nombre,\n    sum(p.total) AS total\nFROM clientes c"
        );
    }

    #[test]
    fn las_clausulas_vuelven_al_margen() {
        let out = format("select id from clientes where estado = 'activo' group by id");
        assert!(out.contains("\nFROM clientes"), "{out}");
        assert!(out.contains("\nWHERE estado"), "{out}");
        assert!(out.contains("\nGROUP BY id"), "{out}");
    }

    #[test]
    fn and_y_or_cuelgan_de_su_where() {
        let out = format(
            "select id from clientes where estado = 'activo' and creado >= now() - interval '30 days'",
        );
        assert!(
            out.contains("WHERE estado = 'activo'\n  AND creado"),
            "{out}"
        );
    }

    #[test]
    fn la_subconsulta_sangra_un_nivel() {
        let out = format("select * from (select id from clientes) as c");
        assert!(
            out.contains("FROM (\n    SELECT\n        id\n    FROM clientes\n) AS c"),
            "{out}"
        );
    }

    #[test]
    fn la_llamada_a_funcion_no_rompe_linea() {
        let out = format("select count(*) from clientes");
        assert!(out.contains("count(*)"), "{out}");
        assert!(!out.contains("count(\n"), "{out}");
    }

    #[test]
    fn un_select_corto_se_queda_en_una_linea() {
        assert_eq!(format("select 1"), "SELECT 1");
        assert_eq!(format("SELECT 1;"), "SELECT 1;");
    }

    #[test]
    fn los_identificadores_no_cambian_de_caso() {
        let out = format(r#"select "Id" from "Clientes""#);
        assert!(out.contains(r#""Id""#), "{out}");
        assert!(out.contains(r#""Clientes""#), "{out}");

        let out = format("select MiColumna from clientes");
        assert!(out.contains("MiColumna"), "{out}");
    }

    #[test]
    fn el_cuerpo_entre_signos_pesos_sale_identico() {
        let cuerpo = "\nBEGIN\n    PERFORM 1;\n    RETURN 2;\nEND;\n";
        let sql = format!("create function f() returns int language plpgsql as $fn${cuerpo}$fn$");
        let out = format(&sql);
        assert!(out.contains(&format!("$fn${cuerpo}$fn$")), "{out}");
    }

    #[test]
    fn las_palabras_clave_adentro_de_una_cadena_no_se_tocan() {
        let out = format("select 'select from where'");
        assert!(out.contains("'select from where'"), "{out}");
    }

    #[test]
    fn el_comentario_de_linea_no_se_traga_lo_que_sigue() {
        let out = format("select 1 -- comentario\n, 2");
        assert!(out.contains("-- comentario"), "{out}");
        assert!(out.contains("2"), "{out}");
    }

    #[test]
    fn formatear_dos_veces_da_lo_mismo() {
        let sql = "select c.id, c.nombre from clientes c where c.estado = 'activo' and c.edad > 18";
        let once = format(sql);
        let twice = format(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn lo_que_no_entiende_lo_devuelve_igual() {
        // Dos punto y coma seguidos entre dos sentencias: la reconstrucción solo pone uno, así que
        // la guarda de equivalencia nota el token que falta y devuelve la entrada intacta en vez de
        // arriesgarse a perder algo.
        let raro = "SELECT 1;; SELECT 2";
        assert_eq!(format(raro), raro);
    }
}
