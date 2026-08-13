//! Árbol de objetos y DDL.

use pgforge_core::conn::{self, CancelSink};
use pgforge_core::ddl::{self, Ddl};
use pgforge_core::introspect::{self, SchemaGraph, SearchHit, TreeNode, TreeOptions};
use pgforge_core::{ProfileId, Result};
use tauri::State;

use crate::state::{AppState, ReadEntry};

/// Cuántas coincidencias se traen si la interfaz no pide otra cosa. Es un techo de lectura, no de
/// pantalla: con más de doscientos nombres parecidos el problema es el patrón, no la lista.
const SEARCH_LIMIT: i64 = 200;

/// Hijos de un nodo del árbol. Con `parent` en `null` devuelve las bases del servidor.
///
/// `request_id` es opcional y solo sirve para poder cancelarla: abrir un esquema con miles de
/// objetos contra un servidor cargado puede tardar, y sin esto la ventana espera sin nada que
/// apretar. La interfaz manda un identificador propio y con ese mismo llama a `read_cancel`.
#[tauri::command]
pub async fn tree_children(
    state: State<'_, AppState>,
    id: ProfileId,
    parent: Option<TreeNode>,
    options: Option<TreeOptions>,
    request_id: Option<String>,
) -> Result<Vec<TreeNode>> {
    let handle = state.manager.require(id).await?;
    let options = options.unwrap_or_default();
    read(&state, id, request_id, async {
        introspect::children(&handle, parent.as_ref(), options).await
    })
    .await
}

/// Corre una lectura dejándola registrada mientras dura, para que `read_cancel` pueda abortarla.
///
/// Sin `request_id` no se registra nada: quien no piensa cancelar no paga ni el registro ni el
/// bloqueo del mapa.
async fn read<T>(
    state: &State<'_, AppState>,
    id: ProfileId,
    request_id: Option<String>,
    work: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    let Some(request_id) = request_id else {
        return work.await;
    };

    let sink = CancelSink::new();
    state.reads.lock().await.insert(
        request_id.clone(),
        ReadEntry {
            profile: id,
            sink: sink.clone(),
        },
    );

    let outcome = conn::cancelable(sink, work).await;
    // Se saca pase lo que pase: una entrada olvidada haría que un `read_cancel` tardío cancelara la
    // conexión que el pool ya le prestó a otra cosa.
    state.reads.lock().await.remove(&request_id);
    outcome
}

/// Aborta una lectura en curso. Sin nada registrado con ese identificador no hace nada: la lectura
/// ya terminó, y eso no es un error.
#[tauri::command]
pub async fn read_cancel(state: State<'_, AppState>, request_id: String) -> Result<()> {
    let entry = state.reads.lock().await.remove(&request_id);
    let Some(entry) = entry else {
        return Ok(());
    };

    let handle = state.manager.require(entry.profile).await?;
    for token in entry.sink.tokens() {
        handle.cancel(&token).await?;
    }
    Ok(())
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
pub async fn object_ddl(
    state: State<'_, AppState>,
    id: ProfileId,
    node: TreeNode,
    request_id: Option<String>,
) -> Result<Ddl> {
    let handle = state.manager.require(id).await?;
    read(&state, id, request_id, async {
        ddl::object_ddl(&handle, &node).await
    })
    .await
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
