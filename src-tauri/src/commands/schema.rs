//! Árbol de objetos y DDL.

use pgforge_core::ddl::{self, Ddl};
use pgforge_core::introspect::{self, SchemaGraph, SearchHit, TreeNode, TreeOptions};
use pgforge_core::{ProfileId, Result};
use tauri::State;

use crate::state::AppState;

/// Cuántas coincidencias se traen si la interfaz no pide otra cosa. Es un techo de lectura, no de
/// pantalla: con más de doscientos nombres parecidos el problema es el patrón, no la lista.
const SEARCH_LIMIT: i64 = 200;

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

/// Busca objetos por nombre en una base. El árbol se carga por niveles, así que filtrar lo ya
/// traído solo encuentra lo que uno ya había abierto.
#[tauri::command]
pub async fn tree_search(
    state: State<'_, AppState>,
    id: ProfileId,
    database: String,
    pattern: String,
    options: Option<TreeOptions>,
    limit: Option<i64>,
) -> Result<Vec<SearchHit>> {
    let handle = state.manager.require(id).await?;
    introspect::search(
        &handle,
        &database,
        &pattern,
        options.unwrap_or_default(),
        limit.unwrap_or(SEARCH_LIMIT),
    )
    .await
}

#[tauri::command]
pub async fn object_ddl(state: State<'_, AppState>, id: ProfileId, node: TreeNode) -> Result<Ddl> {
    let handle = state.manager.require(id).await?;
    ddl::object_ddl(&handle, &node).await
}

/// Guarda el SVG del diagrama en la ruta elegida.
///
/// Vive acá y no en el núcleo a propósito: el SVG lo dibuja la interfaz, no el servidor, así que
/// no hay lógica que llevar al core. Escribir el archivo desde Rust evita sumar el plugin de
/// archivos y su ámbito de permisos por este único caso.
#[tauri::command]
pub fn erd_export_svg(path: String, svg: String) -> Result<()> {
    std::fs::write(path, svg)?;
    Ok(())
}

/// Tablas y claves foráneas de un esquema, para el diagrama. No trae posiciones: el layout es de
/// la interfaz.
#[tauri::command]
pub async fn schema_graph(
    state: State<'_, AppState>,
    id: ProfileId,
    database: String,
    schema: String,
) -> Result<SchemaGraph> {
    let handle = state.manager.require(id).await?;
    introspect::schema_graph(&handle, &database, &schema).await
}
