//! Abreviaturas del editor: `sf` y un tabulador en vez de `SELECT * FROM `.
//!
//! Van a un archivo JSON legible del directorio de configuración, y no a `localStorage` como el
//! tamaño de letra o el alto del editor. La diferencia no es el tamaño del dato sino de quién es:
//! el tamaño de letra lo vuelve a poner cualquiera en dos segundos, y esto lo escribió el usuario
//! —es de la misma familia que [`crate::sql::saved`], lo que decidió conservar—. Guardado así se
//! puede respaldar, mirar y copiar a otra máquina, que es justo lo que se quiere de una lista que
//! costó armar.
//!
//! **La abreviatura es la identidad**, y por eso es única sin distinguir mayúsculas: escribir `sf`
//! tiene que expandir una cosa y no dos. El identificador es aparte para poder cambiarle la
//! abreviatura a una que ya existe sin que se confunda con crear otra.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};

/// Identificador estable de una abreviatura, para poder renombrarla sin perderla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SnippetId(Uuid);

impl SnippetId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SnippetId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SnippetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for SnippetId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Una abreviatura y el texto en el que se expande.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    #[serde(default)]
    pub id: SnippetId,
    /// Lo que se escribe antes del tabulador. Se compara sin distinguir mayúsculas.
    pub abbreviation: String,
    /// El texto que la reemplaza. Los `${}` son huecos por los que se salta con el tabulador; los
    /// interpreta el editor y acá viajan como el usuario los escribió.
    pub body: String,
    /// Para qué sirve, si el usuario quiso anotarlo. Es lo que la lista de sugerencias muestra al
    /// costado, donde el cuerpo entero no entra.
    #[serde(default)]
    pub description: String,
    /// Para agrupar la lista. Vacía = sin categoría. Todo lo que sigue es `#[serde(default)]`: un
    /// `snippets.json` escrito antes de que existieran estos campos tiene que seguir abriendo.
    #[serde(default)]
    pub category: String,
    /// Dónde vale. Vacío = en todos lados.
    #[serde(default)]
    pub scope: SnippetScope,
    /// Combinación que la expande sin escribir su nombre, escrita como en CodeMirror
    /// (`Mod-Alt-1`). Única entre abreviaturas.
    #[serde(default)]
    pub shortcut: Option<String>,
}

/// En qué servidores y bases vale una abreviatura.
///
/// Las dos condiciones se suman (AND): con perfiles y bases a la vez, hace falta estar en uno de
/// esos servidores **y** en una de esas bases. Vacías las dos, vale en todos lados.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SnippetScope {
    pub profile_ids: Vec<String>,
    pub databases: Vec<String>,
}

/// Techos de lo que se acepta al guardar. El de cuerpo vale también para lo importado: un archivo
/// ajeno no puede meter un texto de megabytes en una lista que se carga entera al arrancar.
const MAX_BODY_CHARS: usize = 50_000;
const MAX_CATEGORY_CHARS: usize = 40;
const MAX_SHORTCUT_CHARS: usize = 40;
const MODIFIERS: [&str; 5] = ["Mod", "Ctrl", "Alt", "Shift", "Meta"];

/// Recorta, saca vacíos y repetidos conservando el orden de aparición.
fn clean_list(items: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let item = item.trim();
        if !item.is_empty() && !out.iter().any(|seen| seen == item) {
            out.push(item.to_owned());
        }
    }
    out
}

/// Valida una combinación al estilo CodeMirror y la deja como se compara. `None` = sin atajo.
///
/// Se exige `Mod`, `Ctrl` o `Alt`: una tecla suelta o solo con `Shift` es texto que se escribe, y
/// asignarla a una abreviatura se comería el teclado (mismo criterio que `shortcuts.ts`).
pub fn normalize_shortcut(raw: &str) -> Result<Option<String>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    if raw.chars().count() > MAX_SHORTCUT_CHARS {
        return Err(Error::Config("el atajo es demasiado largo".into()));
    }
    // La tecla puede ser el propio guion (`Mod--`): se separa antes de partir por guiones.
    let (modifiers, key) = match raw.strip_suffix("--") {
        Some(rest) => (rest, "-"),
        None => raw
            .rsplit_once('-')
            .ok_or_else(|| Error::Config(format!("el atajo «{raw}» no tiene modificador")))?,
    };
    if key.is_empty() || key.chars().any(char::is_whitespace) {
        return Err(Error::Config(format!("el atajo «{raw}» no tiene tecla")));
    }
    let mut seen: Vec<&str> = Vec::new();
    for part in modifiers.split('-') {
        if !MODIFIERS.contains(&part) {
            return Err(Error::Config(format!(
                "«{part}» no es un modificador (Mod, Ctrl, Alt, Shift o Meta)"
            )));
        }
        if seen.contains(&part) {
            return Err(Error::Config(format!("el atajo «{raw}» repite «{part}»")));
        }
        seen.push(part);
    }
    if !seen.iter().any(|m| matches!(*m, "Mod" | "Ctrl" | "Alt")) {
        return Err(Error::Config(
            "el atajo necesita Mod, Ctrl o Alt: sin ellos se comería el texto que se escribe"
                .into(),
        ));
    }
    Ok(Some(raw.to_owned()))
}

/// Normaliza y valida todo lo que **no** depende de las otras abreviaturas. La unicidad de la
/// abreviatura y del atajo se mira aparte, contra la lista.
fn validate(snippet: &mut Snippet) -> Result<()> {
    snippet.abbreviation = normalize(&snippet.abbreviation);

    if snippet.abbreviation.is_empty() {
        return Err(Error::Config("la abreviatura no puede estar vacía".into()));
    }
    // Un espacio adentro nunca se podría escribir: la expansión mira la palabra pegada al
    // cursor, y ahí «se lect» son dos palabras.
    if snippet.abbreviation.split_whitespace().count() > 1 {
        return Err(Error::Config(
            "la abreviatura no puede llevar espacios".into(),
        ));
    }
    if snippet.body.trim().is_empty() {
        return Err(Error::Config("el texto a insertar está vacío".into()));
    }
    if snippet.body.chars().count() > MAX_BODY_CHARS {
        return Err(Error::Config(format!(
            "el texto a insertar pasa de {MAX_BODY_CHARS} caracteres"
        )));
    }
    snippet.category = snippet.category.trim().to_owned();
    if snippet.category.chars().count() > MAX_CATEGORY_CHARS {
        return Err(Error::Config(format!(
            "la categoría pasa de {MAX_CATEGORY_CHARS} caracteres"
        )));
    }
    snippet.scope.profile_ids = clean_list(&snippet.scope.profile_ids);
    snippet.scope.databases = clean_list(&snippet.scope.databases);
    snippet.shortcut = match snippet.shortcut.as_deref() {
        Some(raw) => normalize_shortcut(raw)?,
        None => None,
    };
    Ok(())
}

/// Las abreviaturas con las que arranca una instalación nueva.
///
/// Existen porque una lista vacía no enseña la sintaxis de los huecos ni que la función existe: el
/// primero que abra el diálogo ve cómo se escribe una en vez de una pantalla en blanco. Son las
/// cuatro que se escriben todo el tiempo, no un catálogo.
pub fn defaults() -> Vec<Snippet> {
    [
        (
            "sf",
            "SELECT ${*}\nFROM ${tabla}\nWHERE ${}",
            "Consulta base",
        ),
        ("sc", "SELECT count(*) FROM ${tabla}", "Contar filas"),
        (
            "ij",
            "INNER JOIN ${tabla} ON ${tabla}.${id} = ${otra}.${id}",
            "Unir dos tablas",
        ),
        (
            "cte",
            "WITH ${nombre} AS (\n    ${}\n)\nSELECT * FROM ${nombre}",
            "Expresión de tabla común",
        ),
    ]
    .into_iter()
    .map(|(abbreviation, body, description)| Snippet {
        id: SnippetId::new(),
        abbreviation: abbreviation.to_owned(),
        body: body.to_owned(),
        description: description.to_owned(),
        category: "Consultas".to_owned(),
        scope: SnippetScope::default(),
        shortcut: None,
    })
    .collect()
}

/// Deja la abreviatura como se guarda y se compara: sin espacios alrededor.
///
/// No se pasa a minúsculas porque el usuario puede querer verla como la escribió; lo que no
/// distingue mayúsculas es la **comparación**, que es otra cosa.
fn normalize(abbreviation: &str) -> String {
    abbreviation.trim().to_owned()
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

#[derive(Debug)]
pub struct SnippetStore {
    path: PathBuf,
    snippets: Vec<Snippet>,
}

impl SnippetStore {
    /// Lee las abreviaturas del archivo indicado.
    ///
    /// Un archivo inexistente **no** es un almacén vacío sino uno con [`defaults`], y queda escrito
    /// en el acto: si no, borrar la última abreviatura las haría reaparecer en el próximo arranque,
    /// que es exactamente lo que uno no quiere de una lista que acaba de vaciar a propósito.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        match std::fs::read(&path) {
            Ok(bytes) => {
                let snippets = serde_json::from_slice(&bytes).map_err(|e| {
                    Error::Config(format!(
                        "el archivo de abreviaturas {} está corrupto: {e}",
                        path.display()
                    ))
                })?;
                Ok(Self { path, snippets })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let store = Self {
                    path,
                    snippets: defaults(),
                };
                store.persist()?;
                Ok(store)
            }
            Err(e) => Err(Error::Io(e)),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn snippets(&self) -> &[Snippet] {
        &self.snippets
    }

    /// Agrega la abreviatura, o reemplaza la que tenga el mismo identificador.
    ///
    /// Devuelve [`Error::Conflict`] si **otra** ya usa esa abreviatura: pisarla en silencio dejaría
    /// dos expansiones para lo mismo y ganaría una al azar.
    pub fn upsert(&mut self, mut snippet: Snippet) -> Result<()> {
        validate(&mut snippet)?;

        if let Some(otra) = self
            .snippets
            .iter()
            .find(|s| s.id != snippet.id && same(&s.abbreviation, &snippet.abbreviation))
        {
            return Err(Error::Conflict(format!(
                "«{}» ya está en uso",
                otra.abbreviation
            )));
        }

        // Dos abreviaturas con el mismo atajo dejarían una sin forma de dispararse.
        if let Some(shortcut) = &snippet.shortcut {
            if let Some(otra) = self.snippets.iter().find(|s| {
                s.id != snippet.id && s.shortcut.as_deref().is_some_and(|c| same(c, shortcut))
            }) {
                return Err(Error::Conflict(format!(
                    "el atajo {shortcut} ya lo usa «{}»",
                    otra.abbreviation
                )));
            }
        }

        match self.snippets.iter_mut().find(|s| s.id == snippet.id) {
            Some(slot) => *slot = snippet,
            None => self.snippets.push(snippet),
        }
        self.persist()
    }

    /// Suma lo importado. Una sola escritura al final: un archivo de cien abreviaturas no son cien
    /// reescrituras del archivo de configuración.
    ///
    /// El choque es por abreviatura (sin distinguir mayúsculas). Un **atajo** que choca con el de otra
    /// no descarta la abreviatura: se le quita el atajo, que es lo único que no puede convivir.
    pub fn import(&mut self, items: Vec<Snippet>, mode: ImportMode) -> Result<ImportReport> {
        let mut report = ImportReport::default();
        for mut item in items {
            // Los ids de un archivo ajeno no significan nada acá.
            item.id = SnippetId::new();
            let existing = self
                .snippets
                .iter()
                .position(|s| same(&s.abbreviation, &item.abbreviation));

            let Some(at) = existing else {
                if self.shortcut_taken(item.shortcut.as_deref(), None) {
                    item.shortcut = None;
                }
                self.snippets.push(item);
                report.added += 1;
                continue;
            };
            if mode == ImportMode::Skip {
                report
                    .skipped
                    .push(format!("«{}»: ya existe", item.abbreviation));
                continue;
            }
            let current = &self.snippets[at];
            item.id = current.id;
            // Se conserva el atajo que ya tenía salvo que el importado traiga uno libre.
            let own = current.shortcut.clone();
            item.shortcut = match item.shortcut.take() {
                Some(new) if !self.shortcut_taken(Some(&new), Some(current.id)) => Some(new),
                _ => own,
            };
            self.snippets[at] = item;
            report.replaced += 1;
        }
        if report.added + report.replaced > 0 {
            self.persist()?;
        }
        Ok(report)
    }

    /// Si otra abreviatura (distinta de `except`) ya usa ese atajo.
    fn shortcut_taken(&self, shortcut: Option<&str>, except: Option<SnippetId>) -> bool {
        let Some(shortcut) = shortcut else {
            return false;
        };
        self.snippets.iter().any(|s| {
            Some(s.id) != except && s.shortcut.as_deref().is_some_and(|c| same(c, shortcut))
        })
    }

    pub fn remove(&mut self, id: SnippetId) -> Result<()> {
        self.snippets.retain(|s| s.id != id);
        self.persist()
    }

    /// Vuelve a dejar las de fábrica, descartando lo que haya.
    pub fn reset(&mut self) -> Result<()> {
        self.snippets = defaults();
        self.persist()
    }

    /// Escritura atómica, igual que la lista de conexiones: un corte a mitad de camino no puede
    /// dejar el archivo truncado.
    fn persist(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(&self.snippets)
            .map_err(|e| Error::Config(format!("no se pudieron serializar: {e}")))?;

        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

/// Qué hacer con una abreviatura importada cuya abreviatura ya existe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportMode {
    /// Dejar la que está.
    Skip,
    /// Reemplazarla, conservando su identificador.
    Replace,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub added: usize,
    pub replaced: usize,
    /// Cuáles no entraron y por qué, ya escrito para mostrar.
    pub skipped: Vec<String>,
}

const EXPORT_KIND: &str = "pgforge-snippets";
const EXPORT_VERSION: u32 = 1;
/// Un archivo de abreviaturas real pesa unos kilobytes: pasado esto no es uno.
const MAX_IMPORT_BYTES: usize = 1 << 20;
const MAX_IMPORT_SNIPPETS: usize = 500;

/// Lo que viaja en el archivo: sin identificador y sin los perfiles del alcance. Los dos son de
/// **esta** máquina —un UUID de perfil no significa nada en otra—, y llevarlos solo ensucia el
/// archivo y filtra cómo se llaman tus servidores internos.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ExportedSnippet {
    abbreviation: String,
    body: String,
    description: String,
    category: String,
    databases: Vec<String>,
    shortcut: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportFile<'a> {
    kind: &'a str,
    version: u32,
    snippets: Vec<ExportedSnippet>,
}

/// Solo la cabecera, que se lee antes que el resto para rechazar un archivo ajeno o de otra
/// versión sin interpretarlo «más o menos».
#[derive(Deserialize)]
struct ImportHeader {
    kind: Option<String>,
    version: Option<u32>,
}

#[derive(Deserialize)]
struct ImportBody {
    #[serde(default)]
    snippets: Vec<ExportedSnippet>,
}

/// El archivo de exportación. El contador de usos **no** viaja: es del equipo.
pub fn export(snippets: &[Snippet]) -> String {
    let file = ExportFile {
        kind: EXPORT_KIND,
        version: EXPORT_VERSION,
        snippets: snippets
            .iter()
            .map(|s| ExportedSnippet {
                abbreviation: s.abbreviation.clone(),
                body: s.body.clone(),
                description: s.description.clone(),
                category: s.category.clone(),
                databases: s.scope.databases.clone(),
                shortcut: s.shortcut.clone(),
            })
            .collect(),
    };
    serde_json::to_string_pretty(&file).unwrap_or_default()
}

/// Lo leído de un archivo de importación: lo válido, y por qué se omitió lo demás.
#[derive(Debug, Default)]
pub struct ParsedImport {
    pub snippets: Vec<Snippet>,
    pub skipped: Vec<String>,
}

/// Lee un archivo de abreviaturas ajeno. **Entrada no confiable**: tope de tamaño y de cantidad,
/// y la cabecera se verifica antes de mirar el resto —un JSON cualquiera tiene que fallar diciendo
/// que no es de abreviaturas, y uno de otra versión se rechaza en vez de leerse a medias—.
///
/// Una abreviatura inválida se omite y se anota; no aborta las demás. Un atajo mal escrito no
/// invalida la abreviatura: se le quita el atajo, porque lo que importa del archivo es el texto.
pub fn parse_import(json: &str) -> Result<ParsedImport> {
    if json.len() > MAX_IMPORT_BYTES {
        return Err(Error::Config(
            "el archivo es demasiado grande para ser una lista de abreviaturas".into(),
        ));
    }
    let header: ImportHeader = serde_json::from_str(json)
        .map_err(|e| Error::Config(format!("el archivo no es un JSON legible: {e}")))?;
    if header.kind.as_deref() != Some(EXPORT_KIND) {
        return Err(Error::Config(
            "el archivo no es una exportación de abreviaturas de pgForge".into(),
        ));
    }
    match header.version {
        Some(EXPORT_VERSION) => {}
        Some(other) => {
            return Err(Error::Config(format!(
                "el archivo es de un formato más nuevo (versión {other}); esta versión de pgForge \
                 lee la {EXPORT_VERSION}"
            )))
        }
        None => return Err(Error::Config("al archivo le falta la versión".into())),
    }
    let body: ImportBody = serde_json::from_str(json)
        .map_err(|e| Error::Config(format!("el archivo de abreviaturas está mal formado: {e}")))?;
    if body.snippets.len() > MAX_IMPORT_SNIPPETS {
        return Err(Error::Config(format!(
            "el archivo trae {} abreviaturas; el máximo es {MAX_IMPORT_SNIPPETS}",
            body.snippets.len()
        )));
    }

    let mut parsed = ParsedImport::default();
    for item in body.snippets {
        let mut snippet = Snippet {
            id: SnippetId::new(),
            abbreviation: item.abbreviation,
            body: item.body,
            description: item.description,
            category: item.category,
            scope: SnippetScope {
                profile_ids: Vec::new(),
                databases: item.databases,
            },
            shortcut: None,
        };
        let shortcut = item.shortcut;
        match validate(&mut snippet) {
            Ok(()) => {
                // Un atajo inválido se descarta sin tirar la abreviatura.
                snippet.shortcut = shortcut
                    .as_deref()
                    .and_then(|raw| normalize_shortcut(raw).ok().flatten());
                if parsed
                    .snippets
                    .iter()
                    .any(|s| same(&s.abbreviation, &snippet.abbreviation))
                {
                    parsed.skipped.push(format!(
                        "«{}»: repetida dentro del archivo",
                        snippet.abbreviation
                    ));
                } else {
                    parsed.snippets.push(snippet);
                }
            }
            Err(error) => parsed
                .skipped
                .push(format!("«{}»: {error}", snippet.abbreviation)),
        }
    }
    Ok(parsed)
}

/// Cuántas veces se usó una abreviatura y cuándo fue la última.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetUse {
    pub uses: u64,
    /// Segundos desde el epoch.
    pub last_used_at: Option<i64>,
}

/// El contador de usos, en un archivo **aparte** (`snippet-usage.json`).
///
/// No vive en `snippets.json` porque el contador es de este equipo: no debe viajar al exportar ni
/// al copiar la lista a otra máquina, y reescribir la lista entera en cada expansión sería castigar
/// al tabulador con una escritura de disco.
#[derive(Debug)]
pub struct SnippetUsage {
    path: PathBuf,
    uses: HashMap<SnippetId, SnippetUse>,
}

impl SnippetUsage {
    /// No devuelve error a propósito: es un contador, no un dato. Un archivo corrupto o ilegible
    /// se ignora y se empieza de cero, porque la alternativa es que la aplicación no arranque por
    /// no poder leer cuántas veces se usó `sf`.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let uses = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { path, uses }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn all(&self) -> &HashMap<SnippetId, SnippetUse> {
        &self.uses
    }

    pub fn record(&mut self, id: SnippetId, now: i64) -> Result<()> {
        let entry = self.uses.entry(id).or_default();
        entry.uses += 1;
        entry.last_used_at = Some(now);
        self.persist()
    }

    /// Olvida lo de una abreviatura borrada, para que el archivo no acumule fantasmas.
    pub fn forget(&mut self, id: SnippetId) -> Result<()> {
        if self.uses.remove(&id).is_some() {
            self.persist()?;
        }
        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        if !self.uses.is_empty() {
            self.uses.clear();
            self.persist()?;
        }
        Ok(())
    }

    fn persist(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec(&self.uses)
            .map_err(|e| Error::Config(format!("no se pudo serializar el contador: {e}")))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "pgforge-test-snippets-{}-{name}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn snippet(abbreviation: &str, body: &str) -> Snippet {
        Snippet {
            id: SnippetId::new(),
            abbreviation: abbreviation.to_owned(),
            body: body.to_owned(),
            description: String::new(),
            category: String::new(),
            scope: SnippetScope::default(),
            shortcut: None,
        }
    }

    #[test]
    fn un_archivo_inexistente_trae_las_de_fabrica_y_las_deja_escritas() {
        let path = temp_path("primer-arranque");
        let store = SnippetStore::load(&path).unwrap();

        assert!(!store.snippets().is_empty());
        assert!(store.snippets().iter().any(|s| s.abbreviation == "sf"));
        // Escritas ya: si no, vaciar la lista las haría volver en el próximo arranque.
        assert!(path.exists());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn una_lista_vaciada_a_proposito_no_revive() {
        let path = temp_path("vaciar");
        let mut store = SnippetStore::load(&path).unwrap();
        for id in store.snippets().iter().map(|s| s.id).collect::<Vec<_>>() {
            store.remove(id).unwrap();
        }

        let releido = SnippetStore::load(&path).unwrap();
        assert!(releido.snippets().is_empty(), "volvieron las de fábrica");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn guarda_y_relee() {
        let path = temp_path("ida-y-vuelta");
        let mut store = SnippetStore::load(&path).unwrap();
        store.reset().unwrap();

        let mia = snippet("tt", "SELECT * FROM trabajos");
        let id = mia.id;
        store.upsert(mia).unwrap();

        let releido = SnippetStore::load(&path).unwrap();
        let guardada = releido.snippets().iter().find(|s| s.id == id).unwrap();
        assert_eq!(guardada.body, "SELECT * FROM trabajos");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dos_abreviaturas_iguales_no_conviven_aunque_cambien_las_mayusculas() {
        let path = temp_path("conflicto");
        let mut store = SnippetStore::load(&path).unwrap();
        store.reset().unwrap();
        store.upsert(snippet("tt", "uno")).unwrap();

        let error = store.upsert(snippet("TT", "otro")).unwrap_err();
        assert!(
            matches!(error, Error::Conflict(_)),
            "escribir «tt» tiene que expandir una sola cosa: {error:?}"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cambiarle_el_cuerpo_a_la_misma_no_es_un_conflicto() {
        let path = temp_path("reescribir");
        let mut store = SnippetStore::load(&path).unwrap();
        store.reset().unwrap();

        let mut mia = snippet("tt", "antes");
        store.upsert(mia.clone()).unwrap();
        mia.body = "después".to_owned();
        store.upsert(mia.clone()).unwrap();

        let iguales = store
            .snippets()
            .iter()
            .filter(|s| same(&s.abbreviation, "tt"))
            .count();
        assert_eq!(iguales, 1);
        assert_eq!(
            store
                .snippets()
                .iter()
                .find(|s| s.id == mia.id)
                .unwrap()
                .body,
            "después"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rechaza_lo_que_nunca_se_podria_escribir() {
        let path = temp_path("validar");
        let mut store = SnippetStore::load(&path).unwrap();

        assert!(store.upsert(snippet("  ", "algo")).is_err());
        // La expansión mira la palabra pegada al cursor: con un espacio adentro no hay palabra.
        assert!(store.upsert(snippet("se lect", "algo")).is_err());
        assert!(store.upsert(snippet("tt", "   ")).is_err());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn la_abreviatura_se_guarda_sin_los_espacios_de_los_costados() {
        let path = temp_path("normalizar");
        let mut store = SnippetStore::load(&path).unwrap();
        store.reset().unwrap();
        store.upsert(snippet("  tt  ", "algo")).unwrap();

        assert!(store.snippets().iter().any(|s| s.abbreviation == "tt"));
        let _ = std::fs::remove_file(&path);
    }

    fn con_atajo(abbreviation: &str, shortcut: &str) -> Snippet {
        let mut s = snippet(abbreviation, "SELECT 1");
        s.shortcut = Some(shortcut.to_owned());
        s
    }

    #[test]
    fn un_archivo_de_antes_de_los_campos_nuevos_sigue_abriendo() {
        let path = temp_path("viejo");
        std::fs::write(
            &path,
            r#"[{"id":"7d444840-9dc0-11d1-b245-5ffdce74fad2","abbreviation":"sf","body":"SELECT 1","description":"x"}]"#,
        )
        .unwrap();
        let store = SnippetStore::load(&path).unwrap();
        let s = &store.snippets()[0];
        assert_eq!(s.category, "");
        assert_eq!(s.scope, SnippetScope::default());
        assert_eq!(s.shortcut, None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn valida_el_formato_de_un_atajo() {
        for bueno in ["Mod-Alt-1", "Ctrl-Shift-k", "Alt-F5", "Mod--"] {
            assert!(normalize_shortcut(bueno).unwrap().is_some(), "{bueno}");
        }
        assert_eq!(normalize_shortcut("  ").unwrap(), None);
        for malo in [
            "k",         // sin modificador
            "Shift-k",   // solo Shift: es texto que se escribe
            "Mod-",      // sin tecla
            "Mod-Mod-k", // repetido
            "Super-k",   // modificador inexistente
            "Mod-a b",   // espacio en la tecla
        ] {
            assert!(normalize_shortcut(malo).is_err(), "{malo}");
        }
        let largo = format!("Mod-{}", "x".repeat(60));
        assert!(normalize_shortcut(&largo).is_err());
    }

    #[test]
    fn el_atajo_es_unico_entre_abreviaturas() {
        let path = temp_path("atajo-unico");
        let mut store = SnippetStore::load(&path).unwrap();
        store.reset().unwrap();
        store.upsert(con_atajo("aa", "Mod-Alt-K")).unwrap();

        // La tecla se compara sin distinguir mayúsculas: Mod-Alt-K y Mod-Alt-k son la misma.
        let error = store.upsert(con_atajo("bb", "Mod-Alt-k")).unwrap_err();
        assert!(matches!(error, Error::Conflict(_)), "{error:?}");
        // Reescribir la misma con su propio atajo no choca consigo misma.
        let mut misma = store
            .snippets()
            .iter()
            .find(|s| s.abbreviation == "aa")
            .unwrap()
            .clone();
        misma.body = "SELECT 2".into();
        store.upsert(misma).unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn normaliza_categoria_y_alcance() {
        let path = temp_path("alcance");
        let mut store = SnippetStore::load(&path).unwrap();
        store.reset().unwrap();
        let mut s = snippet("zz", "SELECT 1");
        s.category = "  Mis tablas ".into();
        s.scope.databases = vec![" ventas ".into(), "".into(), "ventas".into()];
        s.scope.profile_ids = vec!["p1".into(), "p1".into()];
        store.upsert(s).unwrap();
        let guardada = store
            .snippets()
            .iter()
            .find(|s| s.abbreviation == "zz")
            .unwrap();
        assert_eq!(guardada.category, "Mis tablas");
        assert_eq!(guardada.scope.databases, ["ventas"]);
        assert_eq!(guardada.scope.profile_ids, ["p1"]);

        let mut larga = snippet("yy", "SELECT 1");
        larga.category = "x".repeat(41);
        assert!(store.upsert(larga).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn el_export_no_lleva_ids_ni_perfiles() {
        let mut s = con_atajo("ex", "Mod-Alt-2");
        s.scope.profile_ids = vec!["perfil-secreto".into()];
        s.scope.databases = vec!["ventas".into()];
        let json = export(&[s.clone()]);
        assert!(!json.contains("perfil-secreto"));
        assert!(!json.contains(&s.id.to_string()));
        assert!(json.contains("pgforge-snippets") && json.contains("Mod-Alt-2"));

        let leido = parse_import(&json).unwrap();
        assert_eq!(leido.snippets.len(), 1);
        assert_eq!(leido.snippets[0].scope.databases, ["ventas"]);
        assert!(leido.snippets[0].scope.profile_ids.is_empty());
        assert_ne!(leido.snippets[0].id, s.id);
    }

    #[test]
    fn rechaza_un_archivo_hostil_antes_de_leerlo_entero() {
        assert!(parse_import("esto no es json").is_err());
        assert!(parse_import(r#"{"kind":"otra-cosa","version":1,"snippets":[]}"#).is_err());
        let error = parse_import(r#"{"kind":"pgforge-snippets","version":99,"snippets":[]}"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("99"), "{error}");
        assert!(parse_import(r#"{"kind":"pgforge-snippets","snippets":[]}"#).is_err());

        let muchas: Vec<String> = (0..600)
            .map(|i| format!(r#"{{"abbreviation":"a{i}","body":"x"}}"#))
            .collect();
        let json = format!(
            r#"{{"kind":"pgforge-snippets","version":1,"snippets":[{}]}}"#,
            muchas.join(",")
        );
        assert!(parse_import(&json).is_err());

        let enorme = format!(
            r#"{{"kind":"pgforge-snippets","version":1,"snippets":[{{"abbreviation":"a","body":"{}"}}]}}"#,
            "x".repeat(MAX_IMPORT_BYTES)
        );
        assert!(parse_import(&enorme).is_err());
    }

    #[test]
    fn omite_las_invalidas_sin_abortar_las_demas() {
        let json = r#"{"kind":"pgforge-snippets","version":1,"snippets":[
            {"abbreviation":"ok","body":"SELECT 1","shortcut":"cualquiera"},
            {"abbreviation":"con espacio","body":"x"},
            {"abbreviation":"vacia","body":"  "},
            {"abbreviation":"OK","body":"otra"}
        ]}"#;
        let leido = parse_import(json).unwrap();
        assert_eq!(leido.snippets.len(), 1);
        // El atajo inválido se descarta, no la abreviatura.
        assert_eq!(leido.snippets[0].shortcut, None);
        assert_eq!(leido.skipped.len(), 3);
        let cuerpo_gigante = format!(
            r#"{{"kind":"pgforge-snippets","version":1,"snippets":[{{"abbreviation":"g","body":"{}"}}]}}"#,
            "x".repeat(MAX_BODY_CHARS + 1)
        );
        assert_eq!(parse_import(&cuerpo_gigante).unwrap().skipped.len(), 1);
    }

    #[test]
    fn importar_en_modo_omitir_y_en_modo_reemplazar() {
        let path = temp_path("importar");
        let mut store = SnippetStore::load(&path).unwrap();
        store.reset().unwrap();
        let existente = con_atajo("ee", "Mod-Alt-3");
        let id = existente.id;
        store.upsert(existente).unwrap();

        let mut nueva = snippet("EE", "SELECT 'nuevo'");
        nueva.shortcut = Some("Mod-Alt-4".into());
        let otra = snippet("ff", "SELECT 1");

        let r = store
            .import(vec![nueva.clone(), otra.clone()], ImportMode::Skip)
            .unwrap();
        assert_eq!((r.added, r.replaced, r.skipped.len()), (1, 0, 1));
        assert_eq!(
            store.snippets().iter().find(|s| s.id == id).unwrap().body,
            "SELECT 1"
        );

        let r = store.import(vec![nueva], ImportMode::Replace).unwrap();
        assert_eq!((r.added, r.replaced), (0, 1));
        let reemplazada = store.snippets().iter().find(|s| s.id == id).unwrap();
        assert_eq!(reemplazada.body, "SELECT 'nuevo'");
        // Conserva el identificador y toma el atajo importado porque estaba libre.
        assert_eq!(reemplazada.shortcut.as_deref(), Some("Mod-Alt-4"));

        // Persistió: se relee igual.
        let releido = SnippetStore::load(&path).unwrap();
        assert!(releido.snippets().iter().any(|s| s.abbreviation == "ff"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn un_atajo_importado_que_choca_se_limpia_sin_descartar_la_abreviatura() {
        let path = temp_path("importar-atajo");
        let mut store = SnippetStore::load(&path).unwrap();
        store.reset().unwrap();
        store.upsert(con_atajo("aa", "Mod-Alt-5")).unwrap();

        let r = store
            .import(vec![con_atajo("bb", "Mod-Alt-5")], ImportMode::Skip)
            .unwrap();
        assert_eq!(r.added, 1);
        let bb = store
            .snippets()
            .iter()
            .find(|s| s.abbreviation == "bb")
            .unwrap();
        assert_eq!(bb.shortcut, None);
        let _ = std::fs::remove_file(&path);
    }

    fn temp_usage(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "pgforge-test-usage-{}-{name}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn el_contador_suma_olvida_y_se_relee() {
        let path = temp_usage("contar");
        let mut usage = SnippetUsage::load(&path);
        let a = SnippetId::new();
        let b = SnippetId::new();
        usage.record(a, 100).unwrap();
        usage.record(a, 200).unwrap();
        usage.record(b, 50).unwrap();
        assert_eq!(
            usage.all()[&a],
            SnippetUse {
                uses: 2,
                last_used_at: Some(200)
            }
        );

        let releido = SnippetUsage::load(&path);
        assert_eq!(releido.all()[&a].uses, 2);

        let mut usage = releido;
        usage.forget(a).unwrap();
        assert!(!usage.all().contains_key(&a));
        assert_eq!(SnippetUsage::load(&path).all().len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn un_contador_corrupto_no_impide_arrancar() {
        let path = temp_usage("corrupto");
        std::fs::write(&path, "{{{ esto no es json").unwrap();
        let mut usage = SnippetUsage::load(&path);
        assert!(usage.all().is_empty());
        // Y sigue sirviendo: el primer uso lo reescribe bien.
        usage.record(SnippetId::new(), 1).unwrap();
        assert_eq!(SnippetUsage::load(&path).all().len(), 1);
        let _ = std::fs::remove_file(&path);
    }
}
