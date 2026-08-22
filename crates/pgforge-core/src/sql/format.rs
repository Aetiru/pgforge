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

use super::lex::{lex, Token, TokenKind};
use super::split::split;

/// Ancho de un nivel de sangría.
const INDENT: usize = 4;

/// Formatea un script entero, sentencia por sentencia, separándolas con `;` y una línea en blanco.
///
/// Pura: el mismo texto de entrada da siempre el mismo resultado, sin tocar el servidor.
pub fn format(sql: &str) -> String {
    let statements = split(sql);
    if statements.is_empty() {
        return sql.to_owned();
    }

    let trailing_semicolon = ends_with_semicolon(sql);

    let mut out = String::new();
    let last = statements.len() - 1;
    for (index, statement) in statements.iter().enumerate() {
        if index > 0 {
            out.push_str(";\n\n");
        }
        out.push_str(&format_statement(&statement.text));
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

fn units_of(tokens: &[Token]) -> Vec<Unit> {
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
                            display: pair,
                            role: Role::Clause,
                        });
                        i += 2;
                        continue;
                    }
                }

                if upper == "JOIN" || JOIN_MODIFIERS.contains(&upper.as_str()) {
                    if let Some(end) = join_end(&words, i) {
                        let phrase = words[i..=end]
                            .iter()
                            .map(|t| t.text.to_uppercase())
                            .collect::<Vec<_>>()
                            .join(" ");
                        units.push(Unit::Word {
                            display: phrase,
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
                    upper
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
            matches!(role, Role::Keyword | Role::Plain) || display == "VALUES"
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
    /// Nivel de sangría vigente, en cantidad de niveles (cada uno son `INDENT` espacios).
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
}

impl Printer {
    fn new(oneline: bool) -> Self {
        Printer {
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
        self.out.push_str(&" ".repeat(level * INDENT));
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
                self.raw(",", true, false, true);
                self.newline_to(indent);
            }
            _ => self.raw(",", true, false, true),
        }
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
                    if display == "SELECT" {
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

fn format_statement(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let tokens = lex(trimmed);
    let units = units_of(&tokens);
    if units.is_empty() {
        return trimmed.to_owned();
    }

    let mut printer = Printer::new(!needs_breaking(&units));
    printer.run(&units);
    printer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

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
