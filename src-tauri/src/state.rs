use std::path::PathBuf;

use pgforge_core::{ConnectionManager, ProfileStore};
use tokio::sync::Mutex;

pub struct AppState {
    pub manager: ConnectionManager,
    pub store: Mutex<ProfileStore>,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> pgforge_core::Result<Self> {
        std::fs::create_dir_all(&config_dir)?;
        Ok(Self {
            manager: ConnectionManager::new(),
            store: Mutex::new(ProfileStore::load(config_dir.join("connections.json"))?),
        })
    }
}
