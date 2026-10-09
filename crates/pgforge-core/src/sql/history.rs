//! Historial de consultas.
//!
//! En SQLite y no en un archivo suelto porque el historial se consulta, no se lee entero: buscar
//! «esa consulta que corrí la semana pasada» sobre miles de entradas es lo único que lo hace útil,
//! y para eso hace falta un índice.
//!
//! Se guarda el texto de la consulta, nunca nada del perfil más allá de su identificador: la
//! contraseña vive en el almacén de credenciales del sistema y no tiene por qué pasar por acá.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Versión del esquema del archivo. Subirla obliga a agregar el paso de migración de abajo.
const SCHEMA_VERSION: i64 = 2;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS history (
        id         INTEGER PRIMARY KEY,
        profile_id TEXT    NOT NULL,
        database   TEXT    NOT NULL,
        sql        TEXT    NOT NULL,
        started_at INTEGER NOT NULL,
        seconds    REAL,
        row_count  INTEGER,
        succeeded  INTEGER NOT NULL,
        error      TEXT,
        source     TEXT    NOT NULL DEFAULT 'editor'
    );
    CREATE INDEX IF NOT EXISTS history_reciente ON history (started_at DESC);
";

/// De dónde salió lo que se ejecutó.
///
/// El historial dejó de ser «lo que escribí en el editor» para ser **lo que la aplicación ejecutó
/// contra el servidor**. Un `ALTER TABLE` salido de un diálogo no quedaba en ningún lado que se
/// pudiera consultar después, que es justo la pregunta del día siguiente: qué cambió y cuándo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// Lo escribió el usuario en una pestaña de consulta.
    Editor,
    /// Lo generó un diálogo de la aplicación y se aplicó desde ahí.
    Dialog,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Source::Editor => "editor",
            Source::Dialog => "dialog",
        }
    }

    fn from_str(text: &str) -> Self {
        match text {
            "dialog" => Source::Dialog,
            _ => Source::Editor,
        }
    }
}

/// Lo que se registra al terminar una ejecución.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewEntry {
    /// Por omisión, el editor: es de donde venía todo lo que se registraba antes de que los
    /// diálogos empezaran a anotar lo suyo.
    #[serde(default = "editor_source")]
    pub source: Source,
    pub profile_id: String,
    pub database: String,
    pub sql: String,
    /// Segundos desde el epoch.
    pub started_at: i64,
    pub seconds: f64,
    pub row_count: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    /// De dónde salió: el editor o un diálogo de la aplicación.
    pub source: Source,
    pub profile_id: String,
    pub database: String,
    pub sql: String,
    pub started_at: i64,
    pub seconds: f64,
    pub row_count: Option<i64>,
    pub succeeded: bool,
    pub error: Option<String>,
}

/// Sustituye por `'***'` el literal que sigue a `PASSWORD`: la contraseña de `CREATE`/`ALTER ROLE`
/// y la opción `password` de `CREATE`/`ALTER USER MAPPING` (y de `CREATE SERVER`, que usa la misma
/// forma `OPTIONS (password '…')`).
///
/// Es función pura y se aplica solo al anotar en el historial: la vista previa y la ejecución llevan
/// el SQL entero, porque el servidor lo necesita. El historial, en cambio, es un archivo de texto
/// que se lista, se busca y se copia, y una contraseña ahí dura mucho más que la operación.
///
/// No analiza SQL entero: recorre el texto saltando comentarios, identificadores entrecomillados y
/// literales (con `''`, `E'…\'…'` y `$tag$…$tag$`) para que un `password` dentro de un literal no
/// cuente, y reemplaza el literal que sigue a la palabra. `PASSWORD NULL` no tiene secreto y queda.
/// Si el literal no cierra se redacta hasta el final: ante la duda, se oculta de más.
pub fn redact_secrets(sql: &str) -> String {
    let c: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch == '-' && c.get(i + 1) == Some(&'-') {
            let end = c[i..]
                .iter()
                .position(|&x| x == '\n')
                .map_or(c.len(), |p| i + p);
            out.extend(&c[i..end]);
            i = end;
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            let end = block_comment_end(&c, i);
            out.extend(&c[i..end]);
            i = end;
        } else if ch == '"' {
            let mut j = i + 1;
            while j < c.len() {
                if c[j] == '"' {
                    if c.get(j + 1) == Some(&'"') {
                        j += 2;
                        continue;
                    }
                    j += 1;
                    break;
                }
                j += 1;
            }
            out.extend(&c[i..j]);
            i = j;
        } else if ch == '\'' {
            let end = quoted_end(&c, i, false);
            out.extend(&c[i..end]);
            i = end;
        } else if ch == '$' {
            let end = dollar_end(&c, i).unwrap_or(i + 1);
            out.extend(&c[i..end]);
            i = end;
        } else if ch.is_alphabetic() || ch == '_' {
            let mut j = i + 1;
            while j < c.len() && (c[j].is_alphanumeric() || c[j] == '_' || c[j] == '$') {
                j += 1;
            }
            let word: String = c[i..j].iter().collect();
            if word.eq_ignore_ascii_case("e") && c.get(j) == Some(&'\'') {
                // Cadena con escapes: el literal entero se copia, la `E` no es una palabra.
                let end = quoted_end(&c, j, true);
                out.extend(&c[i..end]);
                i = end;
                continue;
            }
            out.push_str(&word);
            i = j;
            if word.eq_ignore_ascii_case("password") {
                let k = skip_trivia(&c, i);
                if let Some(end) = literal_end(&c, k) {
                    out.extend(&c[i..k]);
                    out.push_str("'***'");
                    i = end;
                }
            }
        } else {
            out.push(ch);
            i += 1;
        }
    }
    out
}

/// Posición justo después del comentario de bloque que empieza en `start` (anidan, como en
/// PostgreSQL).
fn block_comment_end(c: &[char], start: usize) -> usize {
    let (mut depth, mut i) = (0, start);
    while i < c.len() {
        if c[i] == '/' && c.get(i + 1) == Some(&'*') {
            depth += 1;
            i += 2;
        } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    c.len()
}

/// Posición justo después de la cadena entre comillas simples que abre en `start`.
fn quoted_end(c: &[char], start: usize, backslash_escapes: bool) -> usize {
    let mut i = start + 1;
    while i < c.len() {
        match c[i] {
            '\\' if backslash_escapes => i += 2,
            '\'' if c.get(i + 1) == Some(&'\'') => i += 2,
            '\'' => return i + 1,
            _ => i += 1,
        }
    }
    c.len()
}

/// Posición justo después del literal entre dólares (`$$…$$`, `$tag$…$tag$`) que abre en `start`,
/// o `None` si ahí no abre uno (por ejemplo `$1`).
fn dollar_end(c: &[char], start: usize) -> Option<usize> {
    let mut j = start + 1;
    while j < c.len() && (c[j].is_alphanumeric() || c[j] == '_') {
        j += 1;
    }
    if c.get(j) != Some(&'$') || c.get(start + 1).is_some_and(|d| d.is_ascii_digit()) {
        return None;
    }
    let tag = &c[start..=j];
    let mut i = j + 1;
    while i + tag.len() <= c.len() {
        if &c[i..i + tag.len()] == tag {
            return Some(i + tag.len());
        }
        i += 1;
    }
    Some(c.len())
}

/// Salta espacios y comentarios.
fn skip_trivia(c: &[char], mut i: usize) -> usize {
    loop {
        match c.get(i) {
            Some(x) if x.is_whitespace() => i += 1,
            Some('-') if c.get(i + 1) == Some(&'-') => {
                i = c[i..]
                    .iter()
                    .position(|&x| x == '\n')
                    .map_or(c.len(), |p| i + p);
            }
            Some('/') if c.get(i + 1) == Some(&'*') => i = block_comment_end(c, i),
            _ => return i,
        }
    }
}

/// Fin del literal de texto que empieza en `i`, si ahí empieza uno.
fn literal_end(c: &[char], i: usize) -> Option<usize> {
    match c.get(i)? {
        '\'' => Some(quoted_end(c, i, false)),
        'e' | 'E' if c.get(i + 1) == Some(&'\'') => Some(quoted_end(c, i + 1, true)),
        '$' => dollar_end(c, i),
        _ => None,
    }
}

fn editor_source() -> Source {
    Source::Editor
}

pub struct HistoryStore {
    connection: Connection,
}

const SELECT: &str = "SELECT id, profile_id, database, sql, started_at, seconds, row_count,
                             succeeded, error, source
                        FROM history";

impl HistoryStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        crate::private::restrict(path.as_ref())?;
        let connection = Connection::open(path)?;

        // El historial es lo más prescindible de la aplicación: si el proceso se corta a mitad de
        // una escritura, importa mucho más que la base siga abriendo que no perder la última fila.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;

        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let version: i64 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;

        if version < SCHEMA_VERSION {
            self.connection.execute_batch(SCHEMA)?;
            // `SCHEMA` crea la tabla completa, pero no toca una que ya existe: el archivo de quien
            // venía usando la aplicación se migra con el `ALTER`. Se pregunta antes en vez de
            // ignorar el error, para no tapar uno de verdad.
            if !self.has_column("history", "source")? {
                self.connection.execute(
                    "ALTER TABLE history ADD COLUMN source TEXT NOT NULL DEFAULT 'editor'",
                    [],
                )?;
            }
            self.connection
                .pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }

        Ok(())
    }

    fn has_column(&self, table: &str, column: &str) -> Result<bool> {
        let mut statement = self
            .connection
            .prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn record(&self, entry: &NewEntry) -> Result<i64> {
        self.connection.execute(
            "INSERT INTO history
                 (profile_id, database, sql, started_at, seconds, row_count, succeeded, error,
                  source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.profile_id,
                entry.database,
                entry.sql,
                entry.started_at,
                entry.seconds,
                entry.row_count,
                entry.error.is_none(),
                entry.error,
                entry.source.as_str(),
            ],
        )?;

        Ok(self.connection.last_insert_rowid())
    }

    /// Lo último ejecutado, de un servidor o de todos.
    ///
    /// El desempate por `id` no es cosmético: `started_at` está en segundos y dos ejecuciones del
    /// mismo segundo son lo más normal del mundo, así que sin él el orden entre esas dos queda
    /// librado a lo que devuelva SQLite.
    pub fn recent(&self, profile_id: Option<&str>, limit: i64) -> Result<Vec<Entry>> {
        match profile_id {
            Some(profile_id) => self.query(
                &format!(
                    "{SELECT} WHERE profile_id = ?1 ORDER BY started_at DESC, id DESC LIMIT ?2"
                ),
                params![profile_id, limit],
            ),
            None => self.query(
                &format!("{SELECT} ORDER BY started_at DESC, id DESC LIMIT ?1"),
                params![limit],
            ),
        }
    }

    /// Busca por texto dentro de las consultas.
    pub fn search(&self, text: &str, limit: i64) -> Result<Vec<Entry>> {
        // `escape` para que quien busque `100%` no termine buscando cualquier cosa.
        let pattern = format!(
            "%{}%",
            text.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );

        self.query(
            &format!("{SELECT} WHERE sql LIKE ?1 ESCAPE '\\' ORDER BY started_at DESC LIMIT ?2"),
            params![pattern, limit],
        )
    }

    pub fn clear(&self) -> Result<()> {
        self.connection.execute("DELETE FROM history", [])?;
        Ok(())
    }

    fn query(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<Entry>> {
        let mut statement = self.connection.prepare(sql)?;
        let rows = statement.query_map(params, |row| {
            Ok(Entry {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                database: row.get(2)?,
                sql: row.get(3)?,
                started_at: row.get(4)?,
                seconds: row.get(5)?,
                row_count: row.get(6)?,
                succeeded: row.get(7)?,
                error: row.get(8)?,
                source: Source::from_str(&row.get::<_, String>(9)?),
            })
        })?;

        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Error::History(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(sql: &str) -> NewEntry {
        NewEntry {
            source: Source::Editor,
            profile_id: "servidor-1".into(),
            database: "app".into(),
            sql: sql.into(),
            started_at: 1_700_000_000,
            seconds: 0.5,
            row_count: Some(3),
            error: None,
        }
    }

    #[test]
    fn oculta_la_contrasena_de_un_rol() {
        assert_eq!(
            redact_secrets("CREATE ROLE ana LOGIN PASSWORD 's3cr3t' VALID UNTIL 'infinity'"),
            "CREATE ROLE ana LOGIN PASSWORD '***' VALID UNTIL 'infinity'"
        );
        assert_eq!(
            redact_secrets("alter role ana with encrypted password 'x'"),
            "alter role ana with encrypted password '***'"
        );
    }

    #[test]
    fn las_comillas_escapadas_no_cortan_el_literal() {
        let out = redact_secrets("ALTER ROLE a PASSWORD 'it''s a ''secret'' ok'; SELECT 1");
        assert_eq!(out, "ALTER ROLE a PASSWORD '***'; SELECT 1");
        assert!(!out.contains("secret"));
    }

    #[test]
    fn oculta_la_opcion_password_de_un_user_mapping() {
        let out = redact_secrets(
            "CREATE USER MAPPING FOR ana SERVER remoto OPTIONS (user 'ana', Password 'p''w')",
        );
        assert_eq!(
            out,
            "CREATE USER MAPPING FOR ana SERVER remoto OPTIONS (user 'ana', Password '***')"
        );
        let out =
            redact_secrets("ALTER USER MAPPING FOR ana SERVER r OPTIONS (SET password 'zzz')");
        assert!(!out.contains("zzz") && out.contains("'***'"));
    }

    #[test]
    fn entiende_cadenas_con_escapes_y_entre_dolares() {
        assert_eq!(
            redact_secrets(r"ALTER ROLE a PASSWORD E'a\'b'"),
            "ALTER ROLE a PASSWORD '***'"
        );
        assert_eq!(
            redact_secrets("ALTER ROLE a PASSWORD $$un 'secreto'$$ LOGIN"),
            "ALTER ROLE a PASSWORD '***' LOGIN"
        );
        assert_eq!(
            redact_secrets("ALTER ROLE a PASSWORD /* nota */\n  'x'"),
            "ALTER ROLE a PASSWORD /* nota */\n  '***'"
        );
    }

    #[test]
    fn no_toca_lo_que_no_es_una_contrasena() {
        for sql in [
            "ALTER ROLE a PASSWORD NULL",
            "SELECT 'PASSWORD ''x'''",
            "SELECT \"password\", password FROM usuarios WHERE nombre = 'password'",
            "-- PASSWORD 'x'\nSELECT 1",
            "SELECT 1; /* password 'x' */",
            "CREATE ROLE ñandú LOGIN",
        ] {
            assert_eq!(redact_secrets(sql), sql);
        }
    }

    #[test]
    fn un_literal_sin_cerrar_se_oculta_hasta_el_final() {
        assert_eq!(
            redact_secrets("ALTER ROLE a PASSWORD 'sin cerrar"),
            "ALTER ROLE a PASSWORD '***'"
        );
    }

    fn store() -> HistoryStore {
        // En memoria: el test verifica el esquema y las consultas, no el sistema de archivos.
        HistoryStore::open(":memory:").unwrap()
    }

    #[test]
    fn guarda_y_devuelve_lo_ultimo_primero() {
        let store = store();
        store.record(&entry("SELECT 1")).unwrap();
        store
            .record(&NewEntry {
                started_at: 1_700_000_100,
                ..entry("SELECT 2")
            })
            .unwrap();

        let recientes = store.recent(None, 10).unwrap();
        assert_eq!(recientes.len(), 2);
        assert_eq!(recientes[0].sql, "SELECT 2");
        assert!(recientes[0].succeeded);
        assert_eq!(recientes[0].row_count, Some(3));
    }

    #[test]
    fn un_archivo_viejo_se_migra_sin_perder_lo_que_tenia() {
        // El historial de quien ya venía usando la aplicación no tiene la columna del origen. Que
        // abrir la versión nueva le borre lo guardado sería peor que no tener la columna.
        let path =
            std::env::temp_dir().join(format!("pgforge_history_v1_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);

        {
            let viejo = Connection::open(&path).unwrap();
            viejo
                .execute_batch(
                    "CREATE TABLE history (
                         id         INTEGER PRIMARY KEY,
                         profile_id TEXT    NOT NULL,
                         database   TEXT    NOT NULL,
                         sql        TEXT    NOT NULL,
                         started_at INTEGER NOT NULL,
                         seconds    REAL,
                         row_count  INTEGER,
                         succeeded  INTEGER NOT NULL,
                         error      TEXT
                     );
                     -- Con los mismos valores que escribía la versión anterior: `record` nunca
                     -- dejó `seconds` en NULL.
                     INSERT INTO history
                            (profile_id, database, sql, started_at, seconds, row_count, succeeded)
                     VALUES ('servidor-1', 'app', 'SELECT 1', 1700000000, 0.2, 1, 1);",
                )
                .unwrap();
            viejo.pragma_update(None, "user_version", 1).unwrap();
        }

        let store = HistoryStore::open(&path).unwrap();
        let recientes = store.recent(None, 10).unwrap();

        assert_eq!(recientes.len(), 1, "no se pierde lo que ya estaba");
        assert_eq!(
            recientes[0].source,
            Source::Editor,
            "lo viejo es del editor"
        );

        store
            .record(&NewEntry {
                source: Source::Dialog,
                ..entry("ALTER TABLE t ADD COLUMN c int")
            })
            .unwrap();
        assert_eq!(store.recent(None, 1).unwrap()[0].source, Source::Dialog);

        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn separa_por_servidor() {
        let store = store();
        store.record(&entry("SELECT 1")).unwrap();
        store
            .record(&NewEntry {
                profile_id: "servidor-2".into(),
                ..entry("SELECT 2")
            })
            .unwrap();

        assert_eq!(store.recent(Some("servidor-1"), 10).unwrap().len(), 1);
        assert_eq!(store.recent(Some("nadie"), 10).unwrap().len(), 0);
    }

    #[test]
    fn registra_las_que_fallaron() {
        let store = store();
        store
            .record(&NewEntry {
                error: Some("no existe la tabla".into()),
                row_count: None,
                ..entry("SELECT * FROM nada")
            })
            .unwrap();

        let fallida = &store.recent(None, 1).unwrap()[0];
        assert!(
            !fallida.succeeded,
            "una consulta que falló también es historia"
        );
        assert_eq!(fallida.error.as_deref(), Some("no existe la tabla"));
    }

    #[test]
    fn busca_por_texto() {
        let store = store();
        store.record(&entry("SELECT * FROM clientes")).unwrap();
        store.record(&entry("SELECT * FROM ventas")).unwrap();

        assert_eq!(store.search("clientes", 10).unwrap().len(), 1);
        assert_eq!(store.search("SELECT", 10).unwrap().len(), 2);
        assert_eq!(store.search("no está", 10).unwrap().len(), 0);
    }

    #[test]
    fn los_comodines_del_buscador_son_texto() {
        let store = store();
        store.record(&entry("SELECT 100 % 3")).unwrap();
        store.record(&entry("SELECT 1")).unwrap();

        assert_eq!(
            store.search("100 %", 10).unwrap().len(),
            1,
            "el % que escribió el usuario no puede funcionar como comodín"
        );
    }

    #[test]
    fn vaciar_lo_deja_sin_nada() {
        let store = store();
        store.record(&entry("SELECT 1")).unwrap();
        store.clear().unwrap();

        assert!(store.recent(None, 10).unwrap().is_empty());
    }

    #[test]
    fn reabrir_no_pierde_lo_guardado() {
        let archivo =
            std::env::temp_dir().join(format!("pgforge-historial-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&archivo);

        let store = HistoryStore::open(&archivo).unwrap();
        store.record(&entry("SELECT 1")).unwrap();
        drop(store);

        let reabierto = HistoryStore::open(&archivo).unwrap();
        assert_eq!(reabierto.recent(None, 10).unwrap().len(), 1);

        drop(reabierto);
        let _ = std::fs::remove_file(&archivo);
    }
}
