//! Túnel SSH: un forward local sobre una sesión SSH para alcanzar bases que viven tras un bastión.
//!
//! El enfoque es un *port forward* local, no una integración TLS a medida: se abre un `TcpListener`
//! en `127.0.0.1:<puerto efímero>` y cada conexión que llega se empalma, byte a byte, con un canal
//! `direct-tcpip` hacia el destino real. Así el pool de `deadpool` y `tokio-postgres` conectan a un
//! puerto local como si fuera el servidor, sin enterarse del túnel, y el cifrado de PostgreSQL sigue
//! siendo de extremo a extremo: el bastión transporta los bytes TLS pero no puede leerlos.
//!
//! Se usa `russh` (SSH en Rust puro) para no depender del enlace nativo de `libssh2`, que complica
//! el build en las tres plataformas del CI.

use std::sync::{Arc, Mutex};

use russh::client::{self, Config, Handle};
use russh::keys::known_hosts::{check_known_hosts, learn_known_hosts};
use russh::keys::{load_secret_key, Error as KeysError, HashAlg, PrivateKeyWithHashAlg, PublicKey};
use russh::Disconnect;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use super::profile::SshTunnel;
use super::secret::Password;
use crate::error::{Error, Result};

/// Qué hacer cuando la clave del host no está en `known_hosts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyPolicy {
    /// Solo confía en hosts ya registrados en `known_hosts`; uno desconocido detiene la conexión con
    /// un [`Error::SshHostKey`] que lleva la huella, para que la interfaz pida confirmación. Es el
    /// modo normal: no se acepta una clave a ciegas.
    Strict,
    /// Acepta un host desconocido y lo agrega a `known_hosts` (confianza en el primer uso). Se usa
    /// **solo** después de que el usuario confirmó la huella. Una clave que *cambió* respecto de la
    /// registrada sigue siendo un error incluso en este modo: eso es lo que delata un intermediario.
    TrustOnFirstUse,
}

/// Resultado de verificar la clave del host, comunicado desde el handler de `russh` —que solo puede
/// devolver un `bool`— hacia quien abrió la sesión, para traducirlo a un error con significado.
enum HostKeyVerdict {
    Ok,
    /// El host no estaba en `known_hosts`. Lleva la huella para mostrarla.
    Unknown(String),
    /// El host estaba pero con otra clave: posible intermediario.
    Changed(String),
}

/// Handler de `russh`. Su única responsabilidad es verificar la clave del host contra `known_hosts`.
struct ForwardHandler {
    host: String,
    port: u16,
    policy: HostKeyPolicy,
    verdict: Arc<Mutex<HostKeyVerdict>>,
}

impl ForwardHandler {
    fn on_unknown(
        &self,
        key: &PublicKey,
        fingerprint: String,
    ) -> std::result::Result<bool, russh::Error> {
        match self.policy {
            HostKeyPolicy::TrustOnFirstUse => {
                // Si no se puede escribir `known_hosts`, se continúa igual: el usuario ya confió en
                // esta huella, y no poder recordarla no es razón para abortar la conexión.
                let _ = learn_known_hosts(&self.host, self.port, key);
                Ok(true)
            }
            HostKeyPolicy::Strict => {
                *self.verdict.lock().expect("mutex de host key envenenado") =
                    HostKeyVerdict::Unknown(fingerprint);
                Ok(false)
            }
        }
    }
}

impl client::Handler for ForwardHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let fingerprint = server_public_key.fingerprint(HashAlg::Sha256).to_string();
        match check_known_hosts(&self.host, self.port, server_public_key) {
            Ok(true) => Ok(true),
            Ok(false) => self.on_unknown(server_public_key, fingerprint),
            Err(KeysError::KeyChanged { .. }) => {
                *self.verdict.lock().expect("mutex de host key envenenado") =
                    HostKeyVerdict::Changed(fingerprint);
                Ok(false)
            }
            // No se pudo leer `known_hosts` (por ejemplo, no hay directorio home): se trata como
            // desconocido, que con la política estricta también pide confirmación.
            Err(_) => self.on_unknown(server_public_key, fingerprint),
        }
    }
}

/// Método de autenticación que corresponde a un túnel, según lo que traiga el perfil.
///
/// Se separa como función pura porque es la decisión verificable sin un servidor SSH: con clave
/// privada se autentica por clave; sin ella, por contraseña.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthMethod {
    Key,
    Password,
}

fn auth_method(tunnel: &SshTunnel) -> AuthMethod {
    if tunnel.private_key.is_some() {
        AuthMethod::Key
    } else {
        AuthMethod::Password
    }
}

/// Abre y autentica la sesión SSH contra el bastión, traduciendo los errores a variantes claras.
///
/// El `secret` es la frase de la clave privada cuando se autentica por clave, o la contraseña del
/// usuario SSH cuando se autentica por contraseña; nunca es la contraseña de la base.
async fn open_session(
    tunnel: &SshTunnel,
    secret: Option<&Password>,
    policy: HostKeyPolicy,
) -> Result<Handle<ForwardHandler>> {
    let verdict = Arc::new(Mutex::new(HostKeyVerdict::Ok));
    let handler = ForwardHandler {
        host: tunnel.host.clone(),
        port: tunnel.port,
        policy,
        verdict: Arc::clone(&verdict),
    };

    let config = Arc::new(Config {
        nodelay: true,
        ..Default::default()
    });

    let mut session =
        match client::connect(config, (tunnel.host.as_str(), tunnel.port), handler).await {
            Ok(session) => session,
            Err(err) => {
                // Si la conexión se cortó por la verificación de la clave del host, el error genérico de
                // `russh` no dice nada útil: el veredicto guardado sí, y lleva la huella para que la
                // interfaz muestre un cartel de confirmación en vez de un fallo opaco.
                return Err(
                    match &*verdict.lock().expect("mutex de host key envenenado") {
                        HostKeyVerdict::Unknown(fingerprint) => Error::SshHostKey {
                            host: tunnel.host.clone(),
                            fingerprint: fingerprint.clone(),
                            changed: false,
                        },
                        HostKeyVerdict::Changed(fingerprint) => Error::SshHostKey {
                            host: tunnel.host.clone(),
                            fingerprint: fingerprint.clone(),
                            changed: true,
                        },
                        HostKeyVerdict::Ok => Error::Ssh(format!(
                            "no se pudo conectar al bastión SSH {}:{}: {err}",
                            tunnel.host, tunnel.port
                        )),
                    },
                );
            }
        };

    let authenticated = match auth_method(tunnel) {
        AuthMethod::Key => {
            // `private_key` es `Some` por construcción de `auth_method`.
            let path = tunnel.private_key.as_ref().expect("hay clave privada");
            let key = load_secret_key(path, secret.map(Password::expose)).map_err(|e| {
                Error::Ssh(format!(
                    "no se pudo leer la clave privada {}: {e}",
                    path.display()
                ))
            })?;
            // El hash de firma solo importa para claves RSA; para las demás `russh` lo ignora.
            let hash = session
                .best_supported_rsa_hash()
                .await
                .map_err(|e| Error::Ssh(format!("falló la negociación SSH: {e}")))?
                .flatten();
            let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash);
            session
                .authenticate_publickey(&tunnel.user, key)
                .await
                .map_err(|e| Error::Ssh(format!("falló la autenticación por clave: {e}")))?
                .success()
        }
        AuthMethod::Password => {
            let password = secret.map(Password::expose).ok_or_else(|| {
                Error::Ssh(
                    "el túnel SSH necesita una contraseña o una clave privada, y no se dio ninguna"
                        .to_owned(),
                )
            })?;
            session
                .authenticate_password(&tunnel.user, password)
                .await
                .map_err(|e| Error::Ssh(format!("falló la autenticación por contraseña: {e}")))?
                .success()
        }
    };

    if !authenticated {
        return Err(Error::Ssh(
            "el bastión SSH rechazó las credenciales del túnel".to_owned(),
        ));
    }

    Ok(session)
}

/// Empalma una conexión local con un canal `direct-tcpip` hacia el destino, en ambos sentidos.
///
/// Un error de copia no es una falla de la aplicación: es una de las dos puntas cerrando la
/// conexión, que es lo normal al terminar. Por eso el resultado se descarta en quien la llama.
async fn forward_connection(
    session: &Handle<ForwardHandler>,
    mut local: TcpStream,
    originator: std::net::SocketAddr,
    target_host: &str,
    target_port: u16,
) -> Result<()> {
    let channel = session
        .channel_open_direct_tcpip(
            target_host.to_owned(),
            u32::from(target_port),
            originator.ip().to_string(),
            u32::from(originator.port()),
        )
        .await
        .map_err(|e| {
            Error::Ssh(format!(
                "no se pudo abrir el canal SSH hacia {target_host}:{target_port}: {e}"
            ))
        })?;

    let mut remote = channel.into_stream();
    let _ = tokio::io::copy_bidirectional(&mut local, &mut remote).await;
    Ok(())
}

/// Forward local activo. Mientras vive, `local_port()` acepta conexiones y las tuneliza al destino.
///
/// Al soltarse (cuando se desconecta el servidor) aborta el bucle de aceptación y deja caer la
/// sesión SSH; las conexiones ya establecidas se cierran solas cuando el pool las suelta.
pub struct LocalForward {
    local_port: u16,
    accept: JoinHandle<()>,
}

impl LocalForward {
    /// Puerto local al que hay que conectar en vez de al host real.
    pub fn local_port(&self) -> u16 {
        self.local_port
    }
}

impl Drop for LocalForward {
    fn drop(&mut self) {
        self.accept.abort();
    }
}

/// Levanta el forward local: conecta el bastión, se autentica y empieza a aceptar conexiones que
/// tuneliza hacia `target_host:target_port`.
pub async fn open_tunnel(
    tunnel: &SshTunnel,
    secret: Option<&Password>,
    target_host: &str,
    target_port: u16,
    policy: HostKeyPolicy,
) -> Result<LocalForward> {
    let session = Arc::new(open_session(tunnel, secret, policy).await?);

    // El puerto 0 deja que el sistema elija uno libre; se lee después con `local_addr`.
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| Error::Ssh(format!("no se pudo abrir el puerto local del túnel: {e}")))?;
    let local_port = listener
        .local_addr()
        .map_err(|e| Error::Ssh(format!("no se pudo leer el puerto local del túnel: {e}")))?
        .port();

    let target_host = target_host.to_owned();
    let accept = tokio::spawn(async move {
        loop {
            let (socket, originator) = match listener.accept().await {
                Ok(pair) => pair,
                // El listener se cerró o falló: se termina el bucle, no hay nada que aceptar.
                Err(_) => break,
            };
            let session = Arc::clone(&session);
            let target_host = target_host.clone();
            tokio::spawn(async move {
                let _ = forward_connection(&session, socket, originator, &target_host, target_port)
                    .await;
            });
        }
    });

    Ok(LocalForward { local_port, accept })
}

/// Prueba el túnel de punta a punta sin dejarlo abierto: autentica contra el bastión y comprueba que
/// desde ahí se alcanza el destino, abriendo un canal `direct-tcpip` y cerrándolo.
///
/// Sirve para el botón «Probar» del diálogo de conexión, antes de guardar el perfil.
pub async fn test_connection(
    tunnel: &SshTunnel,
    secret: Option<&Password>,
    target_host: &str,
    target_port: u16,
    policy: HostKeyPolicy,
) -> Result<()> {
    let session = open_session(tunnel, secret, policy).await?;
    session
        .channel_open_direct_tcpip(target_host.to_owned(), u32::from(target_port), "127.0.0.1", 0)
        .await
        .map_err(|e| {
            Error::Ssh(format!(
                "el túnel se abrió pero el bastión no pudo alcanzar {target_host}:{target_port}: {e}"
            ))
        })?;
    // El resultado de desconectar no importa: la prueba ya pasó y la sesión se cierra igual al salir.
    let _ = session
        .disconnect(Disconnect::ByApplication, "prueba de túnel", "")
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tunnel(with_key: bool) -> SshTunnel {
        SshTunnel {
            host: "bastion".to_owned(),
            port: 22,
            user: "deploy".to_owned(),
            private_key: with_key.then(|| PathBuf::from("/home/deploy/.ssh/id_ed25519")),
        }
    }

    #[test]
    fn elige_clave_cuando_hay_clave_privada_y_contrasena_si_no() {
        assert_eq!(auth_method(&tunnel(true)), AuthMethod::Key);
        assert_eq!(auth_method(&tunnel(false)), AuthMethod::Password);
    }
}
