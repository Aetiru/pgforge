//! Traer servidores que ya están configurados en otra herramienta.
//!
//! La primera pared de cualquier herramienta nueva es volver a cargar a mano los veinte servidores
//! que uno ya tiene anotados en otro lado. Acá se leen los tres lugares donde suelen estar: los
//! archivos de `libpq` (`~/.pgpass` y `~/.pg_service.conf`), que están en cualquier máquina donde se
//! usó `psql`, y el archivo de fuentes de datos de DBeaver.
//!
//! **Nunca se traen contraseñas.** `.pgpass` las tiene en texto plano y sería un renglón traerlas,
//! pero pgforge las guarda en el almacén de credenciales del sistema y solo cuando el usuario pide
//! recordarlas: copiarlas sin preguntar convertiría un archivo que el usuario decidió tener así en
//! otra copia que no decidió. Se importa a qué servidor conectarse; la contraseña se pide al
//! conectar, como siempre.
//!
//! Leer el archivo y entenderlo están separados a propósito: los parsers son puros y se prueban con
//! el texto de ejemplo de cada formato, que es donde están las trampas —los `\:` escapados de
//! `.pgpass`, los comentarios y los comodines—.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Result;

use super::profile::{ConnectionProfile, Environment};

/// De dónde salió un servidor importado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportOrigin {
    /// `~/.pgpass`, el archivo de contraseñas de `libpq`.
    Pgpass,
    /// `~/.pg_service.conf`, los servicios con nombre de `libpq`.
    Service,
    /// El `data-sources.json` del espacio de trabajo de DBeaver.
    Dbeaver,
}

/// Un servidor encontrado en otra herramienta, listo para agregar.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub origin: ImportOrigin,
    /// De qué archivo salió, para que se pueda decidir sabiendo de dónde viene.
    pub source: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    /// Puede venir vacío: DBeaver guarda el usuario junto con la contraseña, en su archivo cifrado.
    pub user: String,
    /// La carpeta que el servidor tenía allá. DBeaver las usa igual que pgforge, así que se respeta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Para qué sirve el servidor, si la otra herramienta lo decía.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Environment>,
}

impl Candidate {
    /// El perfil que se guardaría. Sin contraseña y sin recordarla: eso lo decide el usuario al
    /// conectar.
    pub fn profile(&self) -> ConnectionProfile {
        let mut profile = ConnectionProfile::new(&self.name, &self.host, &self.user);
        profile.port = self.port;
        profile.database = self.database.clone();
        profile.group = self.group.clone();
        profile.environment = self.environment;
        profile
    }
}

/// Los tres archivos donde puede haber servidores configurados, existan o no.
pub fn sources(home: &Path, app_data: Option<&Path>) -> Vec<PathBuf> {
    let mut out = vec![home.join(".pgpass"), home.join(".pg_service.conf")];

    // En Windows, `libpq` busca `pgpass.conf` bajo `%APPDATA%\postgresql`, y DBeaver guarda su
    // espacio de trabajo también bajo `%APPDATA%`.
    if let Some(data) = app_data {
        out.push(data.join("postgresql").join("pgpass.conf"));
        out.push(data.join("postgresql").join(".pg_service.conf"));
        out.push(
            data.join("DBeaverData")
                .join("workspace6")
                .join("General")
                .join(".dbeaver")
                .join("data-sources.json"),
        );
    }

    out
}

/// Lee los archivos que existan y devuelve lo que haya, sin repetidos.
pub fn scan(paths: &[PathBuf]) -> Result<Vec<Candidate>> {
    let mut out: Vec<Candidate> = Vec::new();

    for path in paths {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let source = path.display().to_string();

        let found = if name.ends_with(".json") {
            dbeaver(&text, &source)
        } else if name.contains("service") {
            services(&text, &source)
        } else {
            pgpass(&text, &source)
        };

        for candidate in found {
            // El mismo servidor puede estar en `.pgpass` y en un servicio: se muestra una vez.
            let repetido = out.iter().any(|item| {
                item.host == candidate.host
                    && item.port == candidate.port
                    && item.user == candidate.user
                    && item.database == candidate.database
            });
            if !repetido {
                out.push(candidate);
            }
        }
    }

    Ok(out)
}

/// `hostname:port:database:username:password`, una por línea.
///
/// Los comodines (`*`) son parte del formato y no un dato: una línea `*:*:*:postgres:secreto` no
/// dice a qué servidor conectarse, así que se descarta. Los `:` de un valor van escapados con `\`.
pub fn pgpass(text: &str, source: &str) -> Vec<Candidate> {
    let mut out = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let fields = split_escaped(line);
        if fields.len() < 4 {
            continue;
        }
        let (host, port, database, user) = (&fields[0], &fields[1], &fields[2], &fields[3]);
        if host == "*" || user == "*" {
            continue;
        }

        out.push(Candidate {
            origin: ImportOrigin::Pgpass,
            source: source.to_owned(),
            name: format!("{host} ({user})"),
            host: host.clone(),
            port: port.parse().unwrap_or(5432),
            // Un comodín en la base es «cualquiera»: se entra por la de mantenimiento y el árbol
            // muestra el resto.
            database: if database == "*" {
                "postgres".to_owned()
            } else {
                database.clone()
            },
            user: user.clone(),
            group: None,
            environment: None,
        });
    }

    out
}

/// `[nombre]` y `clave=valor`, el formato de `pg_service.conf`.
pub fn services(text: &str, source: &str) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    let mut current: Option<(String, Vec<(String, String)>)> = None;

    let cerrar = |servicio: Option<(String, Vec<(String, String)>)>, out: &mut Vec<Candidate>| {
        let Some((name, values)) = servicio else {
            return;
        };
        let get = |key: &str| {
            values
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, value)| value.clone())
        };
        let host = get("host").or_else(|| get("hostaddr"));
        let Some(host) = host else {
            return;
        };

        out.push(Candidate {
            origin: ImportOrigin::Service,
            source: String::new(),
            name,
            host,
            port: get("port").and_then(|p| p.parse().ok()).unwrap_or(5432),
            database: get("dbname").unwrap_or_else(|| "postgres".to_owned()),
            user: get("user").unwrap_or_default(),
            group: None,
            environment: None,
        });
    };

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            cerrar(current.take(), &mut out);
            current = Some((name.trim().to_owned(), Vec::new()));
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            if let Some((_, values)) = current.as_mut() {
                values.push((key.trim().to_lowercase(), value.trim().to_owned()));
            }
        }
    }
    cerrar(current.take(), &mut out);

    for candidate in &mut out {
        candidate.source = source.to_owned();
    }
    out
}

/// El `data-sources.json` de DBeaver: un objeto `connections` con una entrada por servidor.
///
/// Se leen solo las de PostgreSQL —el mismo archivo tiene las de MySQL o SQLite— y se saltean las
/// que no dicen a qué host conectarse.
pub fn dbeaver(text: &str, source: &str) -> Vec<Candidate> {
    let Ok(root) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let Some(connections) = root.get("connections").and_then(Value::as_object) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for (id, entry) in connections {
        let provider = entry
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if provider != "postgresql" {
            continue;
        }

        let configuration = entry.get("configuration");
        let text_of = |key: &str| {
            configuration
                .and_then(|config| config.get(key))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let Some(host) = text_of("host") else {
            continue;
        };

        out.push(Candidate {
            origin: ImportOrigin::Dbeaver,
            source: source.to_owned(),
            name: entry
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_owned(),
            host,
            port: text_of("port")
                .and_then(|port| port.parse().ok())
                .unwrap_or(5432),
            database: text_of("database").unwrap_or_else(|| "postgres".to_owned()),
            // Suele venir vacío: DBeaver guarda el usuario en su archivo de credenciales, cifrado.
            // El diálogo deja completarlo de una vez para todos los que llegan así.
            user: text_of("user").unwrap_or_default(),
            group: entry
                .get("folder")
                .and_then(Value::as_str)
                .map(str::to_owned),
            // DBeaver marca cada conexión como dev/test/prod, igual que pgforge; el color de
            // producción es de las cosas que uno no quiere volver a configurar a mano.
            environment: text_of("type").as_deref().and_then(environment),
        });
    }

    out
}

/// El entorno que dice la otra herramienta, si es uno de los que pgforge conoce.
fn environment(kind: &str) -> Option<Environment> {
    match kind.to_lowercase().as_str() {
        "dev" | "development" => Some(Environment::Dev),
        "test" | "qa" => Some(Environment::Test),
        "prod" | "production" => Some(Environment::Prod),
        _ => None,
    }
}

/// Parte por `:` respetando los `\:` que el formato usa para un dos puntos literal.
fn split_escaped(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut escaped = false;

    for character in line.chars() {
        if escaped {
            field.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == ':' {
            out.push(std::mem::take(&mut field));
        } else {
            field.push(character);
        }
    }
    out.push(field);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_un_pgpass_con_escapes_y_comentarios() {
        let texto = "# el de siempre\n\
                     db.interno:5433:app:alvaro:secreto\n\
                     \n\
                     raro\\:host:5432:app:alvaro:otra\n";
        let found = pgpass(texto, "/home/x/.pgpass");

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].host, "db.interno");
        assert_eq!(found[0].port, 5433);
        assert_eq!(found[0].database, "app");
        assert_eq!(found[0].user, "alvaro");
        // El dos puntos escapado es parte del nombre, no un separador.
        assert_eq!(found[1].host, "raro:host");
    }

    #[test]
    fn una_linea_con_comodines_no_dice_a_que_servidor_conectarse() {
        assert!(pgpass("*:*:*:postgres:secreto", "x").is_empty());
        // Con host concreto sí sirve, aunque la base sea un comodín.
        let found = pgpass("db:5432:*:postgres:secreto", "x");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].database, "postgres");
    }

    #[test]
    fn no_trae_la_contrasena_aunque_este_en_el_archivo() {
        let found = pgpass("db:5432:app:alvaro:secreto", "x");
        let profile = found[0].profile();

        assert!(!profile.save_password);
        // El perfil no tiene dónde guardarla: la contraseña vive en el almacén del sistema.
        let json = serde_json::to_string(&profile).unwrap();
        assert!(
            !json.contains("secreto"),
            "no puede viajar la contraseña: {json}"
        );
    }

    #[test]
    fn lee_los_servicios_con_nombre() {
        let texto = "# servicios\n\
                     [produccion]\n\
                     host=prod.interno\n\
                     port=6432\n\
                     dbname=app\n\
                     user=lector\n\
                     \n\
                     [sin_host]\n\
                     dbname=app\n";
        let found = services(texto, "/home/x/.pg_service.conf");

        assert_eq!(found.len(), 1, "el que no dice host no sirve: {found:?}");
        assert_eq!(found[0].name, "produccion");
        assert_eq!(found[0].host, "prod.interno");
        assert_eq!(found[0].port, 6432);
        assert_eq!(found[0].user, "lector");
    }

    #[test]
    fn lee_las_fuentes_de_dbeaver_y_saltea_las_de_otro_motor() {
        let texto = r#"{
            "connections": {
                "postgres-1": {
                    "provider": "postgresql",
                    "name": "Produccion",
                    "configuration": {
                        "host": "prod.interno",
                        "port": "5432",
                        "database": "app",
                        "user": "alvaro"
                    }
                },
                "mysql-1": {
                    "provider": "mysql",
                    "name": "Otro motor",
                    "configuration": { "host": "otro", "port": "3306" }
                }
            }
        }"#;
        let found = dbeaver(texto, "data-sources.json");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Produccion");
        assert_eq!(found[0].database, "app");
    }

    #[test]
    fn de_dbeaver_se_traen_tambien_la_carpeta_y_el_entorno() {
        // Son las dos cosas que uno no quiere volver a configurar a mano en veinte servidores, y la
        // de producción además pinta el árbol.
        let texto = r#"{
            "connections": {
                "postgres-1": {
                    "provider": "postgresql",
                    "name": "Facturación",
                    "folder": "Clientes/ACME",
                    "configuration": {
                        "host": "prod.interno",
                        "database": "app",
                        "type": "prod"
                    }
                }
            }
        }"#;
        let found = dbeaver(texto, "data-sources.json");

        assert_eq!(found[0].group.as_deref(), Some("Clientes/ACME"));
        assert_eq!(found[0].environment, Some(Environment::Prod));
        // Sin usuario en el archivo: DBeaver lo guarda cifrado aparte.
        assert!(found[0].user.is_empty());
    }

    #[test]
    fn un_archivo_que_no_se_entiende_no_rompe_nada() {
        assert!(dbeaver("{ esto no es json", "x").is_empty());
        assert!(services("cualquier cosa", "x").is_empty());
    }
}
