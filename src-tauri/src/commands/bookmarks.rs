//! Marcadores del árbol: objetos del catálogo que el usuario decidió tener siempre a mano.
//!
//! Los cinco comandos devuelven la lista entera —mismo criterio que `saved_*`/`snippet_*` en
//! `query.rs`: es corta, así la interfaz no la recompone a mano tras cada operación puntual.

use pgforge_core::{Bookmark, BookmarkTarget, Result};
use tauri::State;

use crate::state::AppState;

/// Segundos desde el epoch. Mismo cálculo que `commands::query::epoch_seconds`, repetido acá para
/// no atar este módulo al de consultas por un detalle de reloj.
fn epoch_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

#[tauri::command]
pub async fn bookmarks_list(state: State<'_, AppState>) -> Result<Vec<Bookmark>> {
    state.bookmarks.lock().await.list()
}

/// Marca un objeto. Apretar la estrella sobre algo ya marcado no falla ni duplica (ver
/// `BookmarkStore::add`).
#[tauri::command]
pub async fn bookmark_add(
    state: State<'_, AppState>,
    target: BookmarkTarget,
    label: Option<String>,
) -> Result<Vec<Bookmark>> {
    let store = state.bookmarks.lock().await;
    store.add(
        &pgforge_core::NewBookmark { target, label },
        epoch_seconds(),
    )?;
    store.list()
}

#[tauri::command]
pub async fn bookmark_remove(state: State<'_, AppState>, id: i64) -> Result<Vec<Bookmark>> {
    let store = state.bookmarks.lock().await;
    store.remove(id)?;
    store.list()
}

/// Borra por objeto, para cuando la estrella se apaga desde el árbol y ahí solo se tiene el
/// objeto, no el `id` del marcador.
#[tauri::command]
pub async fn bookmark_remove_target(
    state: State<'_, AppState>,
    target: BookmarkTarget,
) -> Result<Vec<Bookmark>> {
    let store = state.bookmarks.lock().await;
    store.remove_target(&target)?;
    store.list()
}

#[tauri::command]
pub async fn bookmark_label(
    state: State<'_, AppState>,
    id: i64,
    label: Option<String>,
) -> Result<Vec<Bookmark>> {
    let store = state.bookmarks.lock().await;
    store.set_label(id, label.as_deref())?;
    store.list()
}
