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

        match self.snippets.iter_mut().find(|s| s.id == snippet.id) {
            Some(slot) => *slot = snippet,
            None => self.snippets.push(snippet),
        }
        self.persist()
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
}
