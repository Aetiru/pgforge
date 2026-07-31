//! Perfiles de conexión.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::secret::Password;
use crate::error::{Error, Result};

/// Identificador estable de un perfil. Se usa también como clave en el almacén de credenciales del
/// sistema, así que renombrar un perfil no pierde su contraseña.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(Uuid);

impl ProfileId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ProfileId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for ProfileId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Modo de cifrado del canal, con la misma semántica que `libpq`.
///
/// La distinción importante es entre `Require`, que cifra pero no valida el certificado, y
/// `VerifyCa`/`VerifyFull`, que sí lo validan. Presentar ambos como "SSL activado" sería engañoso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SslMode {
    /// Sin cifrado.
    Disable,
    /// Intenta cifrar; si el servidor no lo soporta, continúa en claro.
    #[default]
    Prefer,
    /// Exige cifrado, pero no valida la identidad del servidor.
    Require,
    /// Exige cifrado y valida la cadena del certificado.
    VerifyCa,
    /// Exige cifrado, valida la cadena y que el nombre del servidor coincida.
    VerifyFull,
}

impl SslMode {
    pub fn is_encrypted(self) -> bool {
        !matches!(self, SslMode::Disable)
    }

    pub fn verifies_certificate(self) -> bool {
        matches!(self, SslMode::VerifyCa | SslMode::VerifyFull)
    }
}

/// Túnel SSH.
///
/// Todavía no se establece ninguna conexión con esto: está en el modelo desde el principio porque
/// la mayoría de las bases de producción se alcanzan a través de un bastión, y agregar el campo
/// más adelante obligaría a migrar los archivos de perfiles ya guardados de los usuarios.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshTunnel {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// Ruta a la clave privada. Si es `None`, se usa autenticación por contraseña.
    pub private_key: Option<PathBuf>,
}

/// Datos de un servidor guardado.
///
/// La contraseña no es un campo: vive en el almacén de credenciales del sistema bajo
/// [`ProfileId`], o se pide en cada conexión. Así este struct se puede serializar, loguear y
/// mostrar sin riesgo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfile {
    #[serde(default)]
    pub id: ProfileId,
    pub name: String,
    /// Carpeta bajo la cual agrupar el servidor en el árbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    /// Base a la que se conecta primero. Las demás se descubren al expandir el servidor.
    pub database: String,
    pub user: String,
    #[serde(default)]
    pub ssl_mode: SslMode,
    /// Certificado raíz propio, para servidores con una CA interna.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_cert: Option<PathBuf>,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
    /// `statement_timeout` aplicado a la sesión. `None` deja el valor del servidor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement_timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<SshTunnel>,
    /// Si la contraseña debe guardarse en el almacén de credenciales del sistema.
    #[serde(default)]
    pub save_password: bool,
}

fn default_connect_timeout() -> u64 {
    10
}

impl ConnectionProfile {
    /// Perfil nuevo con los valores por omisión de PostgreSQL.
    pub fn new(name: impl Into<String>, host: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            id: ProfileId::new(),
            name: name.into(),
            group: None,
            host: host.into(),
            port: 5432,
            database: "postgres".to_owned(),
            user: user.into(),
            ssl_mode: SslMode::default(),
            root_cert: None,
            connect_timeout_secs: default_connect_timeout(),
            statement_timeout_ms: None,
            tunnel: None,
            save_password: false,
        }
    }

    /// Descripción corta para mostrar junto al nombre.
    pub fn target(&self) -> String {
        format!(
            "{}@{}:{}/{}",
            self.user, self.host, self.port, self.database
        )
    }

    /// Arma un perfil a partir de una cadena de conexión, en formato URL
    /// (`postgres://usuario:clave@host:5432/base?sslmode=require`) o de pares `clave=valor`.
    ///
    /// El parseo lo hace `tokio-postgres`, que es el mismo que después va a usar la conexión: si
    /// reimplementáramos el formato acá, cualquier diferencia aparecería como un error de conexión
    /// inexplicable.
    pub fn from_url(name: impl Into<String>, url: &str) -> Result<(Self, Option<Password>)> {
        use tokio_postgres::config::Host;

        let config: tokio_postgres::Config = url
            .parse()
            .map_err(|e| Error::Config(format!("cadena de conexión inválida: {e}")))?;

        let host = config
            .get_hosts()
            .iter()
            .find_map(|host| match host {
                Host::Tcp(name) => Some(name.clone()),
                #[cfg(unix)]
                Host::Unix(path) => Some(path.to_string_lossy().into_owned()),
                #[allow(unreachable_patterns)]
                _ => None,
            })
            .unwrap_or_else(|| "localhost".to_owned());

        let password = config
            .get_password()
            .map(|bytes| Password::new(String::from_utf8_lossy(bytes).into_owned()));

        let profile = Self {
            id: ProfileId::new(),
            name: name.into(),
            group: None,
            host,
            port: config.get_ports().first().copied().unwrap_or(5432),
            database: config.get_dbname().unwrap_or("postgres").to_owned(),
            user: config
                .get_user()
                .map(str::to_owned)
                .unwrap_or_else(|| "postgres".to_owned()),
            ssl_mode: match config.get_ssl_mode() {
                tokio_postgres::config::SslMode::Disable => SslMode::Disable,
                tokio_postgres::config::SslMode::Require => SslMode::Require,
                _ => SslMode::Prefer,
            },
            root_cert: None,
            connect_timeout_secs: default_connect_timeout(),
            statement_timeout_ms: None,
            tunnel: None,
            save_password: false,
        };

        Ok((profile, password))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_perfil_serializado_no_contiene_la_contrasena() {
        let profile = ConnectionProfile::new("local", "localhost", "postgres");
        let json = serde_json::to_string(&profile).unwrap();
        assert!(!json.contains("\"password\""));
        assert!(json.contains("\"sslMode\":\"prefer\""));
    }

    #[test]
    fn parsea_una_url_de_conexion() {
        let (profile, password) =
            ConnectionProfile::from_url("test", "postgres://ana:s3cr3t@db.interno:5433/ventas")
                .unwrap();

        assert_eq!(profile.host, "db.interno");
        assert_eq!(profile.port, 5433);
        assert_eq!(profile.user, "ana");
        assert_eq!(profile.database, "ventas");
        assert_eq!(password.unwrap().expose(), "s3cr3t");
        // La contraseña viene aparte justamente para que no quede dentro del perfil.
        assert!(!serde_json::to_string(&profile).unwrap().contains("s3cr3t"));
    }

    #[test]
    fn una_url_sin_partes_opcionales_usa_los_valores_por_omision() {
        let (profile, password) =
            ConnectionProfile::from_url("test", "postgres://localhost").unwrap();
        assert_eq!(profile.port, 5432);
        assert_eq!(profile.database, "postgres");
        assert!(password.is_none());
    }

    #[test]
    fn los_campos_ausentes_toman_valores_por_omision() {
        // Un archivo escrito por una versión anterior, sin los campos agregados después.
        let json = r#"{
            "name": "viejo",
            "host": "db.interno",
            "port": 5432,
            "database": "app",
            "user": "app"
        }"#;
        let profile: ConnectionProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.ssl_mode, SslMode::Prefer);
        assert_eq!(profile.connect_timeout_secs, 10);
        assert!(profile.tunnel.is_none());
    }
}
