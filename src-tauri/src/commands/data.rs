//! Datos de una tabla.
//!
//! La forma de la tabla viaja de ida y de vuelta en vez de guardarse en el estado: describe el
//! catálogo, no una conexión ni una transacción abierta, y así la interfaz puede mostrar la misma
//! grilla después de un refresco sin que el backend tenga que recordar nada entre llamadas.

use pgforge_core::data::{self, Applied, Change, Cursor, Page, Statement, TableShape};
use pgforge_core::{ProfileId, Result};
use tauri::State;

use crate::state::AppState;

/// Columnas y clave de una tabla. Es lo que decide si la grilla se abre editable.
#[tauri::command]
pub async fn data_open(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    oid: u32,
) -> Result<TableShape> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    data::shape(&handle, &database, oid).await
}

/// Una página de filas. Con `cursor` en `null` devuelve la primera.
#[tauri::command]
pub async fn data_page(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    shape: TableShape,
    cursor: Option<Cursor>,
    limit: Option<usize>,
) -> Result<Page> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    data::page(
        &handle,
        &database,
        &shape,
        cursor.as_ref(),
        limit.unwrap_or(data::DEFAULT_PAGE_SIZE),
    )
    .await
}

/// El SQL que se ejecutaría, sin ejecutar nada.
///
/// No toca la red ni el estado: es el núcleo puro expuesto, para que la vista previa muestre
/// exactamente lo que se va a correr y no una reconstrucción parecida.
#[tauri::command]
pub fn data_preview(shape: TableShape, changes: Vec<Change>) -> Result<Vec<Statement>> {
    data::statements(&shape, &changes)
}

/// Aplica los cambios pendientes en una sola transacción.
#[tauri::command]
pub async fn data_apply(
    state: State<'_, AppState>,
    id: ProfileId,
    database: Option<String>,
    shape: TableShape,
    changes: Vec<Change>,
) -> Result<Applied> {
    let handle = state.manager.require(id).await?;
    let database = database.unwrap_or_else(|| handle.default_database().to_owned());

    data::apply(&handle, &database, &shape, &changes).await
}
