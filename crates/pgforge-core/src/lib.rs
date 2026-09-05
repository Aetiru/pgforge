//! Núcleo de pgforge.
//!
//! Este crate no depende de Tauri ni de ninguna interfaz gráfica: todo lo que la aplicación de
//! escritorio puede hacer debe poder hacerse también desde `pgforge-cli`. Si algo solo funciona
//! desde la ventana, pertenece al lugar equivocado.

#![warn(clippy::disallowed_macros)]

pub mod backup;
pub mod bookmarks;
pub mod caps;
pub mod compare;
pub mod conn;
pub mod data;
pub mod ddl;
pub mod error;
pub mod introspect;
pub mod monitor;
pub mod scripts;
pub mod settings;
pub mod sql;
pub mod update;
pub mod workspace;

pub use bookmarks::{Bookmark, BookmarkKind, BookmarkStore, NewBookmark, Target as BookmarkTarget};
pub use caps::{ServerCaps, ServerVersion};
pub use conn::{
    ConnectionManager, ConnectionProfile, Password, ProfileId, ProfileStore, ServerHandle,
};
pub use error::{Error, Result};
pub use introspect::{TreeNode, TreeOptions};
pub use scripts::{ImportReport, ScriptEntry, ScriptFolder};
pub use workspace::{Workspace, WorkspaceId, WorkspaceSource, WorkspaceStore};
