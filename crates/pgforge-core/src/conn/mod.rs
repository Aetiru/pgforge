//! Gestión de conexiones: perfiles guardados, credenciales, cifrado y pools.

pub mod manager;
pub mod profile;
pub mod secret;
pub mod store;
pub mod tls;
pub mod tunnel;

pub use manager::{ConnectionManager, DatabaseInfo, Notice, ServerHandle, Session};
pub use profile::{normalize_group, ConnectionProfile, Environment, ProfileId, SshTunnel, SslMode};
pub use secret::Password;
pub use store::ProfileStore;
pub use tunnel::HostKeyPolicy;
