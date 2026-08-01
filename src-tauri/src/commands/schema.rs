//! Árbol de objetos y DDL.

use pgforge_core::ddl::{self, Ddl};
use pgforge_core::introspect::{self, TreeNode, TreeOptions};
use pgforge_core::{ProfileId, Result};
use tauri::State;

use crate::state::AppState;

/// Hijos de un nodo del árbol. Con `parent` en `null` devuelve las bases del servidor.
#[tauri::command]
pub async fn tree_children(
    state: State<'_, AppState>,
    id: ProfileId,
    parent: Option<TreeNode>,
    options: Option<TreeOptions>,
) -> Result<Vec<TreeNode>> {
    let handle = state.manager.require(id).await?;
    introspect::children(&handle, parent.as_ref(), options.unwrap_or_default()).await
}

#[tauri::command]
pub async fn object_ddl(state: State<'_, AppState>, id: ProfileId, node: TreeNode) -> Result<Ddl> {
    let handle = state.manager.require(id).await?;
    ddl::object_ddl(&handle, &node).await
}
