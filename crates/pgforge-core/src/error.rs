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

    #[error("configuración inválida: {0}")]
    Config(String),

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
    Other {
        message: String,
    },
}

impl From<&Error> for ErrorPayload {
    fn from(err: &Error) -> Self {
        match err {
            Error::Canceled => ErrorPayload::Canceled,
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
