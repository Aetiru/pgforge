//! Workspaces: ventanas propias acotadas a una carpeta de servidores.
//!
//! El modelo entero vive en `pgforge_core::workspace` — acá solo se traduce y se agrega lo único que
//! pide ventana: abrir la ventana de Tauri correspondiente.

use pgforge_core::{Error, Result, Workspace, WorkspaceId, WorkspaceSource};
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::state::AppState;

/// Segundos desde el epoch. Mismo criterio que `commands::query::epoch_seconds`: el reloj lo pone
/// quien tiene el sistema operativo, no el núcleo, para que `WorkspaceStore::create` siga siendo
/// determinístico y se pueda probar sin depender de la hora real.
fn epoch_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

/// Label de la ventana de un workspace. Prefijo `ws-` para distinguirla de la principal (`main`) al
/// decidir, por ejemplo, si un evento de cierre le corresponde.
fn window_label(id: WorkspaceId) -> String {
    format!("ws-{id}")
}

#[tauri::command]
pub async fn workspace_list(state: State<'_, AppState>) -> Result<Vec<Workspace>> {
    Ok(state.workspaces.lock().await.list().to_vec())
}

#[tauri::command]
pub async fn workspace_get(state: State<'_, AppState>, id: WorkspaceId) -> Result<Workspace> {
    state
        .workspaces
        .lock()
        .await
        .get(id)
        .cloned()
        .ok_or_else(|| Error::Config("el workspace no existe".to_owned()))
}

/// Crea un workspace nativo (no importado de otra herramienta) acotado a `root_group`, o a todo el
/// árbol si es `None`.
#[tauri::command]
pub async fn workspace_create(
    state: State<'_, AppState>,
    name: String,
    root_group: Option<String>,
) -> Result<Workspace> {
    state
        .workspaces
        .lock()
        .await
        .create(name, root_group, WorkspaceSource::Native, epoch_seconds())
}

#[tauri::command]
pub async fn workspace_rename(
    state: State<'_, AppState>,
    id: WorkspaceId,
    name: String,
) -> Result<()> {
    state.workspaces.lock().await.rename(id, name)
}

#[tauri::command]
pub async fn workspace_delete(state: State<'_, AppState>, id: WorkspaceId) -> Result<()> {
    state.workspaces.lock().await.delete(id)
}

/// Abre la ventana del workspace, o le da foco si ya estaba abierta. Nunca hay dos ventanas para el
/// mismo workspace: el `label` sale del identificador, así que la segunda llamada encuentra la
/// primera en vez de crear otra.
///
/// Apunta al mismo `index.html` que la ventana principal, con `?workspace=<id>` en la URL — la
/// interfaz decide con eso a qué carpeta acotar el árbol, igual que cualquier otro parámetro que ya
/// lea del lado de la ventana.
#[tauri::command]
pub async fn workspace_open(
    app: AppHandle,
    state: State<'_, AppState>,
    id: WorkspaceId,
) -> Result<()> {
    let workspace = state
        .workspaces
        .lock()
        .await
        .get(id)
        .cloned()
        .ok_or_else(|| Error::Config("el workspace no existe".to_owned()))?;

    let label = window_label(id);
    if let Some(window) = app.get_webview_window(&label) {
        window.set_focus().map_err(|e| {
            Error::Config(format!("no se pudo enfocar la ventana del workspace: {e}"))
        })?;
        return Ok(());
    }

    let url = WebviewUrl::App(format!("index.html?workspace={id}").into());
    WebviewWindowBuilder::new(&app, &label, url)
        .title(&workspace.name)
        .inner_size(1280.0, 820.0)
        .min_inner_size(940.0, 600.0)
        // Mismo valor que la ventana principal (`"dragDropEnabled": false` en `tauri.conf.json`;
        // el método del builder se llama distinto): en Windows el manejador nativo de drag&drop se
        // come los eventos HTML5 que usa el árbol para mover servidores entre carpetas
        // (`TreePanel.svelte`), y esta ventana también muestra ese árbol.
        .drag_and_drop(false)
        .build()
        .map_err(|e| Error::Config(format!("no se pudo abrir la ventana del workspace: {e}")))?;

    Ok(())
}
