//! Aviso de versión nueva.
//!
//! No es el actualizador firmado de Tauri y no pretende serlo: acá no se descarga ni se instala
//! nada. Se le pregunta a la API de releases de GitHub cuál es la última publicada, se la compara
//! con la que está corriendo y, si hay una más nueva, la interfaz ofrece abrir su página. Es lo que
//! se puede sostener sin par de claves de firma ni secreto de CI, y sobre todo lo que no obliga al
//! usuario a confiar en un binario que baja y se reemplaza solo.
//!
//! La comparación —qué release cuenta como más nueva— es una función pura sobre lo que devuelve
//! GitHub, así que se prueba sin red. Lo único que necesita internet es traer esa lista.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// De dónde se leen las releases. Sale del manifiesto y no de una constante escrita a mano: el
/// repositorio ya está declarado ahí, y dos lugares con la misma dirección terminan discrepando.
const REPO_URL: &str = env!("CARGO_PKG_REPOSITORY");

/// Cuántas releases se piden. Alcanza de sobra para encontrar la más nueva publicada: se piden
/// varias —y no `releases/latest`— porque la lista permite descartar borradores y prelanzamientos
/// acá, con una función que los tests pueden ejercitar, en vez de confiar en el criterio de GitHub.
const PER_PAGE: usize = 20;

/// La comprobación es accesoria: si la red está lenta, importa mucho más que la aplicación siga
/// respondiendo que enterarse de la versión nueva en este arranque.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Una release tal como la devuelve la API de GitHub.
///
/// Es el único tipo del proyecto sin `rename_all = "camelCase"`: los nombres son los que manda
/// GitHub, y renombrarlos rompería la deserialización. No cruza el IPC —para eso está `Release`—,
/// pero es público para que los tests puedan armar la carga desde el JSON de verdad.
#[derive(Debug, Clone, Deserialize)]
pub struct GithubRelease {
    /// El tag del que salió, por ejemplo `v0.1.5`.
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    /// Las notas de la release, en Markdown.
    #[serde(default)]
    pub body: Option<String>,
    /// La página de la release, que es lo que se abre en el navegador.
    pub html_url: String,
    #[serde(default)]
    pub published_at: Option<String>,
    /// Un borrador todavía no existe para nadie más que para quien lo escribió.
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
}

/// Una versión publicada, ya filtrada y lista para mostrar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// La versión sin la `v` del tag: `0.1.5`.
    pub version: String,
    pub name: String,
    /// Las notas en Markdown, tal como las escribió quien publicó. La interfaz las muestra como
    /// texto: interpretar Markdown por un cartel que aparece una vez cada varios meses no paga.
    pub notes: String,
    pub url: String,
    pub published_at: Option<String>,
}

/// El resultado de mirar si hay algo nuevo.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    /// La versión que está corriendo.
    pub current: String,
    /// `None` cuando no hay nada más nuevo, que es el caso normal y no un error.
    pub newer: Option<Release>,
}

/// Una versión `mayor.menor.parche`, con prelanzamiento opcional.
///
/// No se usa la caja `semver` porque de todo el estándar hace falta esto: comparar tres números y
/// saber que un `-rc.1` va antes que la versión final.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Lo que sigue al primer `-`: `rc.1` en `1.0.0-rc.1`.
    pub pre: Option<String>,
}

impl Version {
    /// Acepta lo que sea que traiga el tag: `v0.1.5`, `0.1.5`, `0.1.5-rc.1`, y también `0.1` (que
    /// vale como `0.1.0`). Devuelve `None` si no empieza por un número, que es la forma de decir
    /// «este tag no es una versión y no participa de la comparación».
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim().trim_start_matches(['v', 'V']);
        // El `+metadatos` de semver no ordena nada, así que se descarta antes de partir.
        let text = text.split('+').next().unwrap_or(text);

        let (numbers, pre) = match text.split_once('-') {
            Some((numbers, pre)) if !pre.is_empty() => (numbers, Some(pre.to_owned())),
            _ => (text, None),
        };

        let mut parts = numbers.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().unwrap_or("0").parse().ok()?;
        let patch = parts.next().unwrap_or("0").parse().ok()?;
        if parts.next().is_some() {
            return None;
        }

        Some(Version {
            major,
            minor,
            patch,
            pre,
        })
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                // Con los tres números iguales, tener prelanzamiento es ser anterior: `1.0.0-rc.1`
                // viene antes que `1.0.0`. Entre dos prelanzamientos se comparan como texto, que
                // ordena bien los casos que se usan (`alpha` < `beta` < `rc`).
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => a.cmp(b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// La release publicada más nueva que la versión que corre, si hay alguna.
///
/// Se descartan los borradores y los prelanzamientos: quien quiere probar un `rc` lo busca solo, y
/// ofrecérselo a todo el mundo desde un cartel convierte la versión estable en la excepción. Si la
/// versión actual no se puede interpretar —una compilación local con un número raro— no hay contra
/// qué comparar y no se avisa nada, que es preferible a avisar de más.
pub fn newer_than(current: &str, releases: &[GithubRelease]) -> Option<Release> {
    let current = Version::parse(current)?;

    releases
        .iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| Version::parse(&release.tag_name).map(|version| (version, release)))
        .filter(|(version, _)| *version > current)
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(version, release)| Release {
            version: format!("{}.{}.{}", version.major, version.minor, version.patch),
            name: match release.name.as_deref().map(str::trim) {
                Some(name) if !name.is_empty() => name.to_owned(),
                _ => release.tag_name.clone(),
            },
            notes: release.body.clone().unwrap_or_default(),
            url: release.html_url.clone(),
            published_at: release.published_at.clone(),
        })
}

/// `owner/repo` a partir de la dirección del repositorio que declara el manifiesto.
pub fn repo_slug(url: &str) -> Option<&str> {
    let rest = url
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .strip_prefix("https://github.com/")?;

    // Exactamente dos tramos: cualquier otra cosa no es la raíz de un repositorio y armar una URL
    // de API con eso daría un 404 confuso en vez de un error que se entiende.
    let mut parts = rest.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
        return None;
    }

    Some(rest)
}

/// Pregunta a GitHub si hay una versión más nueva que `current`.
///
/// `current` es la versión de la aplicación que llama —`env!("CARGO_PKG_VERSION")` desde la ventana
/// o desde el CLI— y no la de este crate: el núcleo no tiene por qué ser quien decide qué versión
/// está corriendo el usuario.
pub async fn check(current: &str) -> Result<UpdateCheck> {
    let slug = repo_slug(REPO_URL).ok_or_else(|| {
        Error::UpdateCheck(format!(
            "el repositorio declarado en el manifiesto no es uno de GitHub: {REPO_URL}"
        ))
    })?;

    let response = client(current)?
        .get(format!(
            "https://api.github.com/repos/{slug}/releases?per_page={PER_PAGE}"
        ))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| Error::UpdateCheck(e.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        // El caso frecuente es el 403 por límite de pedidos: la API sin autenticar permite 60 por
        // hora y por dirección IP. Decir el código evita que se lea como «no hay versión nueva».
        return Err(Error::UpdateCheck(format!("GitHub respondió {status}")));
    }

    let releases: Vec<GithubRelease> = response
        .json()
        .await
        .map_err(|e| Error::UpdateCheck(format!("respuesta inesperada de GitHub: {e}")))?;

    Ok(UpdateCheck {
        current: current.to_owned(),
        newer: newer_than(current, &releases),
    })
}

fn client(current: &str) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        // El TLS se arma acá y no se deja en manos de `reqwest` para que la elección del proveedor
        // criptográfico siga siendo una sola en todo el proyecto (ver `conn::tls::web_config`).
        .use_preconfigured_tls(crate::conn::tls::web_config()?)
        .timeout(TIMEOUT)
        // GitHub rechaza con 403 los pedidos sin identificar.
        .user_agent(format!("pgforge/{current}"))
        .build()
        .map_err(|e| Error::UpdateCheck(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> GithubRelease {
        GithubRelease {
            tag_name: tag.to_owned(),
            name: None,
            body: None,
            html_url: format!("https://github.com/Aetiru/pgforge/releases/tag/{tag}"),
            published_at: None,
            draft: false,
            prerelease: false,
        }
    }

    #[test]
    fn interpreta_los_tags_de_una_release() {
        assert_eq!(Version::parse("v0.1.5"), Version::parse("0.1.5"));
        assert_eq!(Version::parse("0.1"), Version::parse("0.1.0"));
        assert_eq!(
            Version::parse("1.0.0-rc.1").unwrap().pre.as_deref(),
            Some("rc.1")
        );
        assert_eq!(Version::parse("1.0.0+build.7"), Version::parse("1.0.0"));
        assert!(Version::parse("nightly").is_none());
        assert!(Version::parse("1.2.3.4").is_none());
    }

    #[test]
    fn ordena_por_numero_y_no_por_texto() {
        let diez = Version::parse("0.1.10").unwrap();
        let cuatro = Version::parse("0.1.4").unwrap();
        assert!(diez > cuatro, "0.1.10 es posterior a 0.1.4");

        assert!(Version::parse("0.2.0").unwrap() > diez);
        assert!(Version::parse("1.0.0-rc.1").unwrap() < Version::parse("1.0.0").unwrap());
        assert!(Version::parse("1.0.0-beta").unwrap() < Version::parse("1.0.0-rc.1").unwrap());
    }

    #[test]
    fn elige_la_mas_nueva_de_las_publicadas() {
        let releases = [release("v0.1.4"), release("v0.2.0"), release("v0.1.9")];

        let newer = newer_than("0.1.4", &releases).expect("hay dos posteriores");
        assert_eq!(newer.version, "0.2.0");
        assert_eq!(newer.name, "v0.2.0", "sin nombre propio queda el tag");
    }

    #[test]
    fn no_avisa_cuando_la_que_corre_es_la_ultima() {
        let releases = [release("v0.1.4"), release("v0.1.3")];

        assert!(newer_than("0.1.4", &releases).is_none());
        assert!(
            newer_than("0.2.0", &releases).is_none(),
            "una local adelantada tampoco avisa"
        );
    }

    #[test]
    fn descarta_borradores_y_prelanzamientos() {
        let mut borrador = release("v0.3.0");
        borrador.draft = true;
        let mut candidata = release("v0.2.0");
        candidata.prerelease = true;

        let releases = [borrador, candidata, release("v0.1.5")];

        let newer = newer_than("0.1.4", &releases).expect("queda la estable");
        assert_eq!(newer.version, "0.1.5");
    }

    #[test]
    fn sin_version_interpretable_no_avisa_nada() {
        assert!(newer_than("compilación local", &[release("v9.9.9")]).is_none());
    }

    #[test]
    fn lee_la_carga_tal_como_la_manda_github() {
        // Recortada, con los campos que importan y en el orden en que llegan.
        let payload = r#"[
            {
                "tag_name": "v0.2.0",
                "name": "0.2.0 — consultas guardadas",
                "body": "* Consultas guardadas\n* Aviso de versión nueva",
                "html_url": "https://github.com/Aetiru/pgforge/releases/tag/v0.2.0",
                "published_at": "2026-08-20T12:00:00Z",
                "draft": false,
                "prerelease": false,
                "assets": []
            }
        ]"#;

        let releases: Vec<GithubRelease> = serde_json::from_str(payload).expect("carga válida");
        let newer = newer_than("0.1.4", &releases).expect("es posterior");

        assert_eq!(newer.version, "0.2.0");
        assert_eq!(newer.name, "0.2.0 — consultas guardadas");
        assert!(newer.notes.contains("Consultas guardadas"));
        assert_eq!(newer.published_at.as_deref(), Some("2026-08-20T12:00:00Z"));
    }

    #[test]
    fn saca_el_repositorio_de_la_direccion_del_manifiesto() {
        assert_eq!(
            repo_slug("https://github.com/Aetiru/pgforge"),
            Some("Aetiru/pgforge")
        );
        assert_eq!(
            repo_slug("https://github.com/Aetiru/pgforge.git"),
            Some("Aetiru/pgforge")
        );
        assert_eq!(
            repo_slug("https://github.com/Aetiru/pgforge/"),
            Some("Aetiru/pgforge")
        );
        assert!(repo_slug("https://gitlab.com/Aetiru/pgforge").is_none());
        assert!(repo_slug("https://github.com/Aetiru").is_none());
        assert!(repo_slug("https://github.com/Aetiru/pgforge/releases").is_none());
    }

    #[test]
    fn el_manifiesto_declara_un_repositorio_de_github() {
        // El aviso de versión nueva sale de ahí: si alguien cambia el campo por una dirección que no
        // es de GitHub, esto lo dice acá y no en tiempo de ejecución contra la API.
        assert!(
            repo_slug(REPO_URL).is_some(),
            "{REPO_URL} no es un repositorio de GitHub"
        );
    }
}
