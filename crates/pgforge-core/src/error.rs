//! Errores del núcleo.
//!
//! Los errores que llegan de PostgreSQL se traducen a variantes con significado en lugar de
//! propagarse crudos: una consulta cancelada por el usuario no es una falla, y un permiso que
//! falta merece un mensaje que diga cuál, no un `ERROR: permission denied` sin contexto.

use tokio_postgres::error::{DbError, ErrorPosition, SqlState};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no se pudo establecer la conexión: {0}")]
    Connection(String),

    #[error("la conexión con el servidor se cerró")]
    ConnectionClosed,

    #[error("versión de PostgreSQL no soportada: {found}. pgforge requiere {min} o superior")]
    UnsupportedVersion { found: String, min: String },

    #[error("permiso insuficiente: {0}")]
    Permission(String),

    #[error("la operación fue cancelada")]
    Canceled,

    /// Los datos cambiaron entre que se leyeron y se quisieron escribir. No es una falla del
    /// servidor ni del usuario, y merece un mensaje distinto de ambos.
    #[error("{0}")]
    Conflict(String),

    /// Cualquier otro error reportado por el servidor, con los campos del protocolo preservados
    /// para que la interfaz pueda resaltar la posición exacta dentro de la consulta.
    #[error("[{code}] {message}")]
    Database {
        code: String,
        message: String,
        detail: Option<String>,
        hint: Option<String>,
        /// Posición del error dentro del texto de la consulta, en caracteres y con base 1.
        position: Option<u32>,
    },

    #[error("no se pudo acceder al almacén de credenciales del sistema: {0}")]
    Credentials(String),

    #[error("no se pudo usar el historial de consultas: {0}")]
    History(String),

    #[error("configuración inválida: {0}")]
    Config(String),

    #[error("no se pudo abrir el túnel SSH: {0}")]
    Ssh(String),

    /// La clave del host SSH no se pudo verificar contra `known_hosts`: o no estaba registrada, o
    /// —peor— cambió respecto de la registrada. No es una falla de red ni de credenciales: la
    /// interfaz muestra la huella y pide confirmación antes de confiar, en vez de aceptarla a ciegas.
    #[error("la clave del host SSH {host} no está verificada (huella {fingerprint})")]
    SshHostKey {
        host: String,
        fingerprint: String,
        /// `true` si el host ya era conocido pero con otra clave: la señal de un posible intermediario.
        changed: bool,
    },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    fn from_db_error(db: &DbError) -> Self {
        let code = db.code();

        if *code == SqlState::QUERY_CANCELED {
            return Error::Canceled;
        }

        if *code == SqlState::INSUFFICIENT_PRIVILEGE {
            return Error::Permission(db.message().to_owned());
        }

        Error::Database {
            code: code.code().to_owned(),
            message: db.message().to_owned(),
            detail: db.detail().map(str::to_owned),
            hint: db.hint().map(str::to_owned),
            position: match db.position() {
                Some(ErrorPosition::Original(p)) => Some(*p),
                // La posición interna apunta a una consulta generada por el servidor (por ejemplo
                // el cuerpo de una función), no al texto que escribió el usuario: señalarla en el
                // editor marcaría el lugar equivocado.
                _ => None,
            },
        }
    }
}

impl From<tokio_postgres::Error> for Error {
    fn from(err: tokio_postgres::Error) -> Self {
        if let Some(db) = err.as_db_error() {
            return Error::from_db_error(db);
        }
        if err.is_closed() {
            return Error::ConnectionClosed;
        }
        Error::Connection(err.to_string())
    }
}

impl From<keyring::Error> for Error {
    fn from(err: keyring::Error) -> Self {
        Error::Credentials(err.to_string())
    }
}

/// Representación serializable para cruzar el IPC de Tauri.
///
/// `thiserror` no implementa `Serialize`, y la interfaz necesita distinguir una cancelación de un
/// error real para no mostrar un cartel rojo cuando el usuario apretó "cancelar".
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ErrorPayload {
    Canceled,
    Conflict {
        message: String,
    },
    Permission {
        message: String,
    },
    #[serde(rename_all = "camelCase")]
    Database {
        code: String,
        message: String,
        detail: Option<String>,
        hint: Option<String>,
        position: Option<u32>,
    },
    /// Clave de host SSH sin verificar. Va aparte de `Other` para que la interfaz la distinga y
    /// muestre la huella en un diálogo de confirmación en lugar de un cartel de error.
    #[serde(rename_all = "camelCase")]
    SshHostKey {
        host: String,
        fingerprint: String,
        changed: bool,
    },
    Other {
        message: String,
    },
}

impl From<&Error> for ErrorPayload {
    fn from(err: &Error) -> Self {
        match err {
            Error::Canceled => ErrorPayload::Canceled,
            Error::Conflict(message) => ErrorPayload::Conflict {
                message: message.clone(),
            },
            Error::Permission(message) => ErrorPayload::Permission {
                message: message.clone(),
            },
            Error::Database {
                code,
                message,
                detail,
                hint,
                position,
            } => ErrorPayload::Database {
                code: code.clone(),
                message: message.clone(),
                detail: detail.clone(),
                hint: hint.clone(),
                position: *position,
            },
            Error::SshHostKey {
                host,
                fingerprint,
                changed,
            } => ErrorPayload::SshHostKey {
                host: host.clone(),
                fingerprint: fingerprint.clone(),
                changed: *changed,
            },
            other => ErrorPayload::Other {
                message: other.to_string(),
            },
        }
    }
}

impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        ErrorPayload::from(self).serialize(serializer)
    }
}
