//! Gestión de conexiones: perfiles guardados, credenciales, cifrado y pools.

pub mod manager;
pub mod profile;
pub mod secret;
pub mod store;
pub mod tls;

pub use manager::{ConnectionManager, ServerHandle};
pub use profile::{ConnectionProfile, ProfileId, SshTunnel, SslMode};
pub use secret::Password;
pub use store::ProfileStore;
