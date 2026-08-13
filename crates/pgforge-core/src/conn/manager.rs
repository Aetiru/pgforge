//! Conexiones vivas.
//!
//! Un servidor conectado mantiene un pool por cada base que se haya abierto: el árbol de objetos
//! salta entre bases del mismo servidor y reconectar en cada salto haría el explorador inutilizable.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::{Manager, ManagerConfig, Object, Pool, PoolError, RecyclingMethod};
use futures_util::{stream, StreamExt};
use serde::Serialize;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_postgres::tls::TlsStream;
use tokio_postgres::{AsyncMessage, CancelToken, Client, Connection, NoTls, Socket};

use super::profile::{ConnectionProfile, ProfileId, SslMode};
use super::secret::Password;
use super::tls;
use super::tunnel::{self, HostKeyPolicy, LocalForward};
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

/// Configuración de conexión común al pool y a las sesiones dedicadas.
///
/// `statement_timeout` se pasa aparte porque no todos los usos quieren el mismo: el explorador
/// respeta el del perfil, el monitoreo impone uno corto, y una tarea de mantenimiento no puede
/// tener ninguno o el servidor mataría el `VACUUM` a mitad de camino.
///
/// `host`/`port` son el destino **efectivo**, no siempre el del perfil: cuando hay túnel SSH apuntan
/// al puerto local del forward, y la conexión llega al servidor real a través del bastión.
fn build_config(
    profile: &ConnectionProfile,
    password: Option<&Password>,
    host: &str,
    port: u16,
    database: &str,
    statement_timeout_ms: Option<u64>,
) -> tokio_postgres::Config {
    let mut cfg = tokio_postgres::Config::new();
    cfg.host(host)
        .port(port)
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

    // Los parámetros de sesión se aplican en el arranque en vez de con un `SET` posterior, así
    // también cubren a las conexiones que el pool recicla. Van todos juntos en un solo llamado
    // porque `options` reemplaza lo anterior: dos llamados dejarían solo el último.
    let mut options = Vec::new();
    if let Some(ms) = statement_timeout_ms {
        options.push(format!("-c statement_timeout={ms}"));
    }
    // Que el rechazo lo haga el servidor es lo que hace de esto una garantía y no un recordatorio:
    // vale igual para el explorador, el editor de SQL, la importación y el mantenimiento.
    if profile.read_only {
        options.push("-c default_transaction_read_only=on".to_owned());
    }
    if !options.is_empty() {
        cfg.options(options.join(" "));
    }

    cfg
}

/// Destino efectivo de una conexión: el host y puerto a los que de verdad se conecta el cliente.
///
/// Sin túnel coincide con el del perfil. Con túnel apunta al puerto local del forward y baja
/// `verify_hostname`, porque la conexión termina en `127.0.0.1` y el nombre del certificado del
/// servidor real nunca coincidiría con esa dirección.
struct Endpoint {
    host: String,
    port: u16,
    verify_hostname: bool,
}

impl Endpoint {
    /// Destino efectivo de un servidor: el del forward si hay túnel, el del perfil si no.
    fn resolve(profile: &ConnectionProfile, tunnel: Option<&LocalForward>) -> Self {
        match tunnel {
            Some(forward) => Endpoint {
                host: "127.0.0.1".to_owned(),
                port: forward.local_port(),
                verify_hostname: false,
            },
            None => Endpoint {
                host: profile.host.clone(),
                port: profile.port,
                verify_hostname: true,
            },
        }
    }
}

fn build_pool(
    profile: &ConnectionProfile,
    password: Option<&Password>,
    endpoint: &Endpoint,
    database: &str,
) -> Result<Pool> {
    let cfg = build_config(
        profile,
        password,
        &endpoint.host,
        endpoint.port,
        database,
        profile.statement_timeout_ms,
    );

    let manager_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };

    let manager = match tls::connector(profile, endpoint.verify_hostname)? {
        Some(connector) => Manager::from_config(cfg, connector, manager_config),
        None => Manager::from_config(cfg, NoTls, manager_config),
    };

    Pool::builder(manager)
        .max_size(POOL_MAX_SIZE)
        .build()
        .map_err(|e| Error::Connection(format!("no se pudo crear el pool: {e}")))
}

/// Mensaje informativo enviado por el servidor durante una operación.
///
/// `VACUUM VERBOSE` y `RAISE NOTICE` reportan su avance por acá, no en el resultado de la
/// consulta: sin capturarlos, una tarea larga es una pantalla en blanco hasta que termina.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub severity: String,
    pub message: String,
}

/// Conexión propia, fuera del pool.
///
/// El monitoreo y el mantenimiento no pueden compartir conexión con el resto de la aplicación: un
/// `VACUUM` de diez minutos dejaría al explorador sin conexiones libres, y para cancelar una
/// consulta hace falta un canal distinto del que la está ejecutando.
pub struct Session {
    client: Client,
    cancel: CancelToken,
    notices: Option<mpsc::UnboundedReceiver<Notice>>,
}

impl Session {
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Permite pedirle al servidor que aborte lo que esta sesión esté ejecutando. Se puede usar
    /// mientras la consulta corre, que es justamente cuando el cliente está ocupado.
    pub fn cancel_token(&self) -> CancelToken {
        self.cancel.clone()
    }

    /// Entrega el canal de mensajes del servidor. Solo el primer llamado lo obtiene.
    pub fn take_notices(&mut self) -> Option<mpsc::UnboundedReceiver<Notice>> {
        self.notices.take()
    }
}

/// Pone a correr la mitad "conexión" del par que devuelve tokio-postgres y deriva sus mensajes
/// asincrónicos a un canal. Sin esta tarea, el cliente no avanza.
fn spawn_connection<T>(client: Client, mut connection: Connection<Socket, T>) -> Session
where
    T: TlsStream + Unpin + Send + 'static,
{
    let cancel = client.cancel_token();
    let (sender, receiver) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        let mut messages = stream::poll_fn(move |cx| connection.poll_message(cx));
        while let Some(message) = messages.next().await {
            match message {
                Ok(AsyncMessage::Notice(notice)) => {
                    let _ = sender.send(Notice {
                        severity: notice.severity().to_owned(),
                        message: notice.message().to_owned(),
                    });
                }
                Ok(_) => {}
                // La conexión se cerró: la tarea termina y quien tenga el cliente verá el error.
                Err(_) => break,
            }
        }
    });

    Session {
        client,
        cancel,
        notices: Some(receiver),
    }
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

tokio::task_local! {
    /// Dónde deja `client()` el token de la conexión que entrega, cuando alguien lo está esperando.
    static CANCEL_SINK: CancelSink;
}

/// Las conexiones que usó una lectura, para poder abortarla desde afuera.
///
/// Una consulta del árbol o un DDL de lectura no tienen sesión propia como la pestaña de consulta:
/// toman una conexión del pool, la usan y la devuelven. Sin esto, una lectura contra un catálogo
/// enorme deja la ventana esperando sin nada que apretar. Se guardan **todos** los tokens porque
/// una sola operación puede pedir más de una conexión —los hijos de una base son cuatro consultas—
/// y cancelar solo la primera dejaría corriendo a las demás.
#[derive(Clone, Default)]
pub struct CancelSink {
    tokens: Arc<std::sync::Mutex<Vec<CancelToken>>>,
}

impl CancelSink {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&self, token: CancelToken) {
        if let Ok(mut tokens) = self.tokens.lock() {
            tokens.push(token);
        }
    }

    /// Los tokens anotados hasta ahora.
    pub fn tokens(&self) -> Vec<CancelToken> {
        self.tokens
            .lock()
            .map(|tokens| tokens.clone())
            .unwrap_or_default()
    }
}

/// Corre una lectura anotando en `sink` las conexiones que vaya usando.
///
/// Vale para cualquier función del núcleo que pida sus conexiones con `ServerHandle::client`: no
/// hace falta que sepa nada de cancelación ni cambiarle la firma.
pub async fn cancelable<T>(sink: CancelSink, future: impl std::future::Future<Output = T>) -> T {
    CANCEL_SINK.scope(sink, future).await
}

/// Un servidor conectado.
pub struct ServerHandle {
    pub profile: ConnectionProfile,
    pub caps: ServerCaps,
    password: Option<Password>,
    /// Forward SSH activo, si el perfil tiene túnel. Un solo túnel por servidor, compartido por los
    /// pools de todas sus bases. Al soltarse el handle se cierra, cerrando el túnel con él.
    tunnel: Option<LocalForward>,
    pools: Mutex<HashMap<String, Pool>>,
}

impl ServerHandle {
    /// Base a la que apunta el perfil, usada cuando no se indica otra.
    pub fn default_database(&self) -> &str {
        &self.profile.database
    }

    /// Destino efectivo al que conectan sus pools y sesiones: el del túnel si lo hay, el del perfil
    /// si no.
    fn endpoint(&self) -> Endpoint {
        Endpoint::resolve(&self.profile, self.tunnel.as_ref())
    }

    /// Credencial con la que se abrió la conexión, para las herramientas externas que necesitan
    /// autenticarse por su cuenta (`pg_dump`, `pg_restore`). Es `pub(crate)` a propósito: fuera
    /// del núcleo nadie tiene por qué tocar la contraseña.
    pub(crate) fn password(&self) -> Option<&Password> {
        self.password.as_ref()
    }

    /// Toma una conexión de la base indicada, abriendo el pool correspondiente la primera vez.
    ///
    /// Si la llamada corre adentro de un [`cancelable`], el token de la conexión entregada se anota
    /// ahí: es lo que permite abortar después una lectura que ya está esperando al servidor.
    pub async fn client(&self, database: &str) -> Result<Object> {
        let pool = {
            let mut pools = self.pools.lock().await;
            match pools.get(database) {
                Some(pool) => pool.clone(),
                None => {
                    let pool = build_pool(
                        &self.profile,
                        self.password.as_ref(),
                        &self.endpoint(),
                        database,
                    )?;
                    pools.insert(database.to_owned(), pool.clone());
                    pool
                }
            }
        };
        let client = pool.get().await.map_err(map_pool_error)?;
        // Fuera de un `cancelable` no hay dónde anotarlo y el `try_with` falla en silencio, que es
        // exactamente lo que corresponde: la mayoría de las lecturas no se cancelan.
        let _ = CANCEL_SINK.try_with(|sink| sink.push(client.cancel_token()));
        Ok(client)
    }

    /// Abre una conexión propia, fuera del pool.
    ///
    /// `statement_timeout_ms` en `None` deja la sesión sin límite, que es lo que corresponde para
    /// las tareas de mantenimiento.
    pub async fn open_session(
        &self,
        database: &str,
        statement_timeout_ms: Option<u64>,
    ) -> Result<Session> {
        let endpoint = self.endpoint();
        let config = build_config(
            &self.profile,
            self.password.as_ref(),
            &endpoint.host,
            endpoint.port,
            database,
            statement_timeout_ms,
        );

        let session = match tls::connector(&self.profile, endpoint.verify_hostname)? {
            Some(connector) => {
                let (client, connection) = config.connect(connector).await?;
                spawn_connection(client, connection)
            }
            None => {
                let (client, connection) = config.connect(NoTls).await?;
                spawn_connection(client, connection)
            }
        };

        Ok(session)
    }

    /// Pide al servidor que aborte lo que esté ejecutando la sesión dueña del token.
    ///
    /// La cancelación viaja por una conexión nueva —la original está ocupada— y esa conexión tiene
    /// que usar el mismo cifrado que el perfil, o un servidor que exige SSL la va a rechazar.
    pub async fn cancel(&self, token: &CancelToken) -> Result<()> {
        // La cancelación abre una conexión nueva; a través de un túnel también viaja por el forward,
        // porque el token guarda la dirección local a la que conectó la sesión original.
        match tls::connector(&self.profile, self.endpoint().verify_hostname)? {
            Some(connector) => token.cancel_query(connector).await?,
            None => token.cancel_query(NoTls).await?,
        }
        Ok(())
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
        self.connect_with_ssh(profile, password, None, HostKeyPolicy::Strict)
            .await
    }

    /// Igual que [`ConnectionManager::connect`], pero permite pasar el secreto del túnel SSH y la
    /// política de verificación de la clave del host.
    ///
    /// El túnel, si el perfil lo trae, se levanta **antes** de construir el pool: recién con el
    /// forward abierto se conoce el puerto local al que apuntan las conexiones. El túnel queda dentro
    /// del `ServerHandle`, así que vive lo mismo que el servidor y se cierra cuando este se cierra.
    pub async fn connect_with_ssh(
        &self,
        profile: ConnectionProfile,
        password: Option<Password>,
        ssh_secret: Option<Password>,
        host_key_policy: HostKeyPolicy,
    ) -> Result<Arc<ServerHandle>> {
        let tunnel = match &profile.tunnel {
            Some(spec) => Some(
                tunnel::open_tunnel(
                    spec,
                    ssh_secret.as_ref(),
                    &profile.host,
                    profile.port,
                    host_key_policy,
                )
                .await?,
            ),
            None => None,
        };

        let endpoint = Endpoint::resolve(&profile, tunnel.as_ref());
        let database = profile.database.clone();
        let pool = build_pool(&profile, password.as_ref(), &endpoint, &database)?;

        let caps = {
            let client = pool.get().await.map_err(map_pool_error)?;
            fetch_caps(&client).await?
        };

        let id = profile.id;
        let handle = Arc::new(ServerHandle {
            profile,
            caps,
            password,
            tunnel,
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
