//! Conexiones vivas.
//!
//! Un servidor conectado mantiene un pool por cada base que se haya abierto: el árbol de objetos
//! salta entre bases del mismo servidor y reconectar en cada salto haría el explorador inutilizable.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::{Manager, ManagerConfig, Object, Pool, PoolError, RecyclingMethod};
use tokio::sync::{Mutex, RwLock};
use tokio_postgres::{Client, NoTls};

use super::profile::{ConnectionProfile, ProfileId, SslMode};
use super::secret::Password;
use super::tls;
use crate::caps::{ServerCaps, ServerVersion, MIN_SUPPORTED_VERSION_NUM};
use crate::error::{Error, Result};

/// Conexiones simultáneas por base. El explorador hace consultas cortas y concurrentes al expandir
/// nodos; más allá de esto solo se consumen backends del servidor sin ganar nada.
const POOL_MAX_SIZE: usize = 6;

fn map_pool_error(err: PoolError) -> Error {
    match err {
        PoolError::Backend(e) => Error::from(e),
        PoolError::Timeout(_) => {
            Error::Connection("se agotó el tiempo esperando una conexión libre del pool".to_owned())
        }
        other => Error::Connection(other.to_string()),
    }
}

fn build_pool(
    profile: &ConnectionProfile,
    password: Option<&Password>,
    database: &str,
) -> Result<Pool> {
    let mut cfg = tokio_postgres::Config::new();
    cfg.host(&profile.host)
        .port(profile.port)
        .user(&profile.user)
        .dbname(database)
        .application_name("pgforge")
        .connect_timeout(Duration::from_secs(profile.connect_timeout_secs));

    if let Some(password) = password {
        cfg.password(password.expose());
    }

    cfg.ssl_mode(match profile.ssl_mode {
        SslMode::Disable => tokio_postgres::config::SslMode::Disable,
        SslMode::Prefer => tokio_postgres::config::SslMode::Prefer,
        // Para el protocolo, los tres modos que exigen cifrado son el mismo; lo que cambia entre
        // ellos es cuánto se valida el certificado, y de eso se ocupa el verificador de `tls`.
        SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull => {
            tokio_postgres::config::SslMode::Require
        }
    });

    // Se aplica en el arranque de la sesión en vez de con un `SET` posterior, así también cubre a
    // las conexiones que el pool recicla.
    if let Some(ms) = profile.statement_timeout_ms {
        cfg.options(format!("-c statement_timeout={ms}"));
    }

    let manager_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };

    let manager = match tls::connector(profile)? {
        Some(connector) => Manager::from_config(cfg, connector, manager_config),
        None => Manager::from_config(cfg, NoTls, manager_config),
    };

    Pool::builder(manager)
        .max_size(POOL_MAX_SIZE)
        .build()
        .map_err(|e| Error::Connection(format!("no se pudo crear el pool: {e}")))
}

const CAPS_SQL: &str = "
    SELECT current_user::text,
           current_database()::text,
           (SELECT rolsuper FROM pg_catalog.pg_roles WHERE rolname = current_user),
           pg_catalog.pg_has_role(current_user, 'pg_signal_backend', 'USAGE'),
           pg_catalog.pg_has_role(current_user, 'pg_read_all_stats', 'USAGE')
";

async fn fetch_caps(client: &Client) -> Result<ServerCaps> {
    // La versión se lee sola y primero: si el servidor es demasiado viejo, cualquier consulta que
    // venga después puede referirse a columnas del catálogo que todavía no existen.
    let row = client
        .query_one("SELECT current_setting('server_version_num')::int4", &[])
        .await?;
    let version = ServerVersion::from_num(row.get::<_, i32>(0));

    if !version.is_supported() {
        return Err(Error::UnsupportedVersion {
            found: version.to_string(),
            min: ServerVersion::from_num(MIN_SUPPORTED_VERSION_NUM).to_string(),
        });
    }

    let row = client.query_one(CAPS_SQL, &[]).await?;
    Ok(ServerCaps {
        version,
        current_user: row.get(0),
        current_database: row.get(1),
        is_superuser: row.get::<_, Option<bool>>(2).unwrap_or(false),
        can_signal_backends: row.get(3),
        can_read_all_stats: row.get(4),
    })
}

/// Un servidor conectado.
pub struct ServerHandle {
    pub profile: ConnectionProfile,
    pub caps: ServerCaps,
    password: Option<Password>,
    pools: Mutex<HashMap<String, Pool>>,
}

impl ServerHandle {
    /// Base a la que apunta el perfil, usada cuando no se indica otra.
    pub fn default_database(&self) -> &str {
        &self.profile.database
    }

    /// Credencial con la que se abrió la conexión, para las herramientas externas que necesitan
    /// autenticarse por su cuenta (`pg_dump`, `pg_restore`). Es `pub(crate)` a propósito: fuera
    /// del núcleo nadie tiene por qué tocar la contraseña.
    pub(crate) fn password(&self) -> Option<&Password> {
        self.password.as_ref()
    }

    /// Toma una conexión de la base indicada, abriendo el pool correspondiente la primera vez.
    pub async fn client(&self, database: &str) -> Result<Object> {
        let pool = {
            let mut pools = self.pools.lock().await;
            match pools.get(database) {
                Some(pool) => pool.clone(),
                None => {
                    let pool = build_pool(&self.profile, self.password.as_ref(), database)?;
                    pools.insert(database.to_owned(), pool.clone());
                    pool
                }
            }
        };
        pool.get().await.map_err(map_pool_error)
    }

    /// Bases del servidor a las que el usuario puede conectarse.
    pub async fn databases(&self) -> Result<Vec<DatabaseInfo>> {
        let client = self.client(self.default_database()).await?;
        let rows = client
            .query(
                "SELECT d.datname::text,
                        pg_catalog.pg_get_userbyid(d.datdba)::text,
                        pg_catalog.pg_encoding_to_char(d.encoding)::text,
                        d.datistemplate,
                        pg_catalog.has_database_privilege(d.oid, 'CONNECT')
                   FROM pg_catalog.pg_database d
                  WHERE d.datallowconn
                  ORDER BY d.datname",
                &[],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| DatabaseInfo {
                name: row.get(0),
                owner: row.get(1),
                encoding: row.get(2),
                is_template: row.get(3),
                can_connect: row.get(4),
            })
            .collect())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    pub name: String,
    pub owner: String,
    pub encoding: String,
    pub is_template: bool,
    pub can_connect: bool,
}

/// Registro de los servidores conectados en esta sesión.
#[derive(Default)]
pub struct ConnectionManager {
    servers: RwLock<HashMap<ProfileId, Arc<ServerHandle>>>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Abre la conexión y calcula las capacidades del servidor. Conectar dos veces el mismo perfil
    /// reemplaza la conexión anterior.
    pub async fn connect(
        &self,
        profile: ConnectionProfile,
        password: Option<Password>,
    ) -> Result<Arc<ServerHandle>> {
        let database = profile.database.clone();
        let pool = build_pool(&profile, password.as_ref(), &database)?;

        let caps = {
            let client = pool.get().await.map_err(map_pool_error)?;
            fetch_caps(&client).await?
        };

        let id = profile.id;
        let handle = Arc::new(ServerHandle {
            profile,
            caps,
            password,
            pools: Mutex::new(HashMap::from([(database, pool)])),
        });

        self.servers.write().await.insert(id, handle.clone());
        Ok(handle)
    }

    pub async fn get(&self, id: ProfileId) -> Option<Arc<ServerHandle>> {
        self.servers.read().await.get(&id).cloned()
    }

    /// Igual que [`ConnectionManager::get`], pero con un error explicable en vez de `None`.
    pub async fn require(&self, id: ProfileId) -> Result<Arc<ServerHandle>> {
        self.get(id)
            .await
            .ok_or_else(|| Error::Config("el servidor no está conectado".to_owned()))
    }

    pub async fn disconnect(&self, id: ProfileId) {
        self.servers.write().await.remove(&id);
    }

    pub async fn connected(&self) -> Vec<ProfileId> {
        self.servers.read().await.keys().copied().collect()
    }
}
