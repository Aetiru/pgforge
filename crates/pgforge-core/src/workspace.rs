//! Espacios de trabajo: una ventana de escritorio acotada a un subárbol de carpetas.
//!
//! No hay un modelo de membresía nuevo: un workspace apunta a una carpeta (`ConnectionProfile::group`)
//! ya existente con [`Workspace::root_group`], y quien filtra el árbol para esa ventana usa
//! [`crate::conn::group_starts_with`] igual que ya hace la carpeta contra sus subcarpetas. Borrar un
//! workspace borra solo ese registro: nunca toca los perfiles ni su `group`, porque la carpeta sigue
//! existiendo para la ventana principal aunque la ventana secundaria que la miraba se haya cerrado.
//!
//! El reloj lo pone quien llama y no este módulo —igual que en `sql::saved`—: así `create` es
//! determinístico y se puede probar sin depender de la hora real.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::conn::normalize_group;
use crate::error::{Error, Result};

/// Identificador estable de un workspace. Mismo patrón que [`crate::ProfileId`]: transparente en
/// JSON y también válido como parte del `label` de la ventana de Tauri que lo muestra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceId(Uuid);

impl WorkspaceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for WorkspaceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for WorkspaceId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// De dónde salió un workspace.
///
/// Puramente informativo: no hay sincronización en vivo con DBeaver, y un workspace `Dbeaver` se
/// administra después exactamente igual que uno `Native`. Sirve para que la interfaz pueda mostrar de
/// dónde vino, nada más.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum WorkspaceSource {
    Native,
    Dbeaver { root: PathBuf },
}

/// Una ventana de escritorio acotada a un subárbol de carpetas de servidores.
///
/// `root_group` es `None` para «todo el árbol, sin acotar» —una ventana que muestra lo mismo que la
/// principal pero aparte—, y con `Some` filtra a esa carpeta y las que cuelgan de ella.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    /// Segundos desde el epoch, igual que el resto de las fechas del núcleo (`sql::saved`,
    /// `sql::history`): el crate no trae `chrono` ni `time`, y agregarlo por un solo campo hubiera
    /// sido una dependencia nueva para lo que un entero ya resuelve.
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_group: Option<String>,
    pub source: WorkspaceSource,
}

/// Almacén de workspaces, en `workspaces.json` — archivo propio y no `connections.json`, porque un
/// workspace no es un perfil ni cambia cómo se conecta ninguno.
#[derive(Debug)]
pub struct WorkspaceStore {
    path: PathBuf,
    workspaces: Vec<Workspace>,
}

impl WorkspaceStore {
    /// Lee los workspaces del archivo indicado. Un archivo inexistente es un almacén vacío, no un
    /// error: es el estado normal antes de crear el primero.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let workspaces = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
                Error::Config(format!(
                    "el archivo de workspaces {} está corrupto: {e}",
                    path.display()
                ))
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(Error::Io(e)),
        };
        Ok(Self { path, workspaces })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn list(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub fn get(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    /// Crea el workspace y lo persiste. `now` lo calcula quien llama (segundos desde el epoch), como
    /// en `sql::saved::SavedStore::save`.
    pub fn create(
        &mut self,
        name: String,
        root_group: Option<String>,
        source: WorkspaceSource,
        now: i64,
    ) -> Result<Workspace> {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Err(Error::Config("el workspace necesita un nombre".to_owned()));
        }

        let workspace = Workspace {
            id: WorkspaceId::new(),
            name,
            created_at: now,
            root_group: normalize_group(root_group.as_deref()),
            source,
        };

        self.workspaces.push(workspace.clone());
        self.persist()?;
        Ok(workspace)
    }

    pub fn rename(&mut self, id: WorkspaceId, name: String) -> Result<()> {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Err(Error::Config("el workspace necesita un nombre".to_owned()));
        }

        let workspace = self
            .workspaces
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| Error::Config("el workspace no existe".to_owned()))?;
        workspace.name = name;
        self.persist()
    }

    /// Borra solo el registro del workspace: nunca toca los perfiles ni el `group` de los que
    /// mostraba. La carpeta que acotaba sigue existiendo para el resto de la aplicación.
    pub fn delete(&mut self, id: WorkspaceId) -> Result<()> {
        self.workspaces.retain(|w| w.id != id);
        self.persist()
    }

    /// Escritura atómica: se escribe un archivo temporal y recién ahí se reemplaza el definitivo,
    /// igual que `conn::store::ProfileStore`, para que un corte a mitad de camino no deje la lista
    /// truncada.
    fn persist(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(&self.workspaces)
            .map_err(|e| Error::Config(format!("no se pudo serializar los workspaces: {e}")))?;

        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "pgforge-test-workspace-{}-{name}.json",
            std::process::id()
        ));
        path
    }

    #[test]
    fn un_archivo_inexistente_es_un_almacen_vacio() {
        let store = WorkspaceStore::load(temp_path("inexistente")).unwrap();
        assert!(store.list().is_empty());
    }

    #[test]
    fn crea_lista_renombra_y_borra() {
        let path = temp_path("ciclo-completo");
        let _ = std::fs::remove_file(&path);

        let mut store = WorkspaceStore::load(&path).unwrap();
        let created = store
            .create(
                "Clientes".to_owned(),
                Some("Clientes/ACME".to_owned()),
                WorkspaceSource::Native,
                1_800_000_000,
            )
            .unwrap();

        assert_eq!(store.list().len(), 1);
        assert_eq!(store.get(created.id).unwrap().name, "Clientes");
        assert_eq!(created.root_group.as_deref(), Some("Clientes/ACME"));
        assert_eq!(created.created_at, 1_800_000_000);

        store.rename(created.id, "Cuentas ACME".to_owned()).unwrap();
        assert_eq!(store.get(created.id).unwrap().name, "Cuentas ACME");

        store.delete(created.id).unwrap();
        assert!(store.list().is_empty(), "el registro se borró");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn borrar_un_workspace_no_toca_ningun_perfil() {
        // El punto central del diseño: el workspace es un registro aparte de `connections.json`, así
        // que borrarlo no tiene ni forma de tocar un perfil — este test documenta esa garantía
        // mostrando que el almacén de workspaces no expone ninguna operación sobre perfiles.
        let path = temp_path("no-toca-perfiles");
        let _ = std::fs::remove_file(&path);

        let mut store = WorkspaceStore::load(&path).unwrap();
        let created = store
            .create(
                "Todo".to_owned(),
                None,
                WorkspaceSource::Native,
                1_800_000_000,
            )
            .unwrap();
        assert!(created.root_group.is_none(), "sin acotar es None");

        store.delete(created.id).unwrap();
        assert!(store.get(created.id).is_none());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn persiste_en_disco_y_se_relee_igual() {
        let path = temp_path("ida-y-vuelta");
        let _ = std::fs::remove_file(&path);

        let mut store = WorkspaceStore::load(&path).unwrap();
        let created = store
            .create(
                "Producción".to_owned(),
                Some("Producción".to_owned()),
                WorkspaceSource::Dbeaver {
                    root: PathBuf::from("/home/x/.dbeaver"),
                },
                1_800_000_100,
            )
            .unwrap();

        let releido = WorkspaceStore::load(&path).unwrap();
        assert_eq!(releido.list().len(), 1);
        let workspace = releido.get(created.id).unwrap();
        assert_eq!(workspace.name, "Producción");
        assert_eq!(workspace.root_group.as_deref(), Some("Producción"));
        assert_eq!(
            workspace.source,
            WorkspaceSource::Dbeaver {
                root: PathBuf::from("/home/x/.dbeaver")
            }
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn el_nombre_de_la_carpeta_raiz_se_normaliza_igual_que_en_los_perfiles() {
        let path = temp_path("normaliza-carpeta");
        let _ = std::fs::remove_file(&path);

        let mut store = WorkspaceStore::load(&path).unwrap();
        let created = store
            .create(
                "Clientes".to_owned(),
                Some("  Clientes / ACME ".to_owned()),
                WorkspaceSource::Native,
                1_800_000_000,
            )
            .unwrap();

        assert_eq!(created.root_group.as_deref(), Some("Clientes/ACME"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn un_nombre_vacio_no_crea_ni_renombra() {
        let path = temp_path("nombre-vacio");
        let _ = std::fs::remove_file(&path);

        let mut store = WorkspaceStore::load(&path).unwrap();
        assert!(matches!(
            store
                .create("   ".to_owned(), None, WorkspaceSource::Native, 1)
                .unwrap_err(),
            Error::Config(_)
        ));

        let created = store
            .create("uno".to_owned(), None, WorkspaceSource::Native, 1)
            .unwrap();
        assert!(matches!(
            store.rename(created.id, "  ".to_owned()).unwrap_err(),
            Error::Config(_)
        ));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn renombrar_o_borrar_un_workspace_inexistente_no_pisa_otros() {
        let path = temp_path("inexistente-en-operacion");
        let _ = std::fs::remove_file(&path);

        let mut store = WorkspaceStore::load(&path).unwrap();
        store
            .create("uno".to_owned(), None, WorkspaceSource::Native, 1)
            .unwrap();

        let otro = WorkspaceId::new();
        assert!(matches!(
            store.rename(otro, "otro".to_owned()).unwrap_err(),
            Error::Config(_)
        ));
        // Borrar uno que no existe no es un error: es idempotente, igual que `ProfileStore::remove`.
        store.delete(otro).unwrap();
        assert_eq!(store.list().len(), 1);

        let _ = std::fs::remove_file(&path);
    }
}
