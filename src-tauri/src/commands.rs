//! Tauri commands exposed to the Svelte frontend via `invoke()`.
//!
//! Each command is a thin async wrapper over RuntimeBackend.
//! Errors serialise as strings (BackendError implements Serialize).

use std::sync::Arc;
use tauri::State;

use crate::backend::{BackendError, RuntimeBackend};
use pelagos_protocol::ContainerInfo;

/// Return all containers (running + exited).
///
/// Frontend: `await invoke('list_containers')`
#[tauri::command]
pub async fn list_containers(
    backend: State<'_, Arc<dyn RuntimeBackend>>,
) -> Result<Vec<ContainerInfo>, BackendError> {
    backend.list_containers().await
}

/// Stop a running container by name.
///
/// Frontend: `await invoke('stop_container', { name: 'my-app' })`
#[tauri::command]
pub async fn stop_container(
    name: String,
    backend: State<'_, Arc<dyn RuntimeBackend>>,
) -> Result<(), BackendError> {
    backend.stop_container(&name).await
}

/// Remove a container.  Pass `force: true` to stop-then-remove.
///
/// Frontend: `await invoke('remove_container', { name: 'my-app', force: false })`
#[tauri::command]
pub async fn remove_container(
    name: String,
    force: bool,
    backend: State<'_, Arc<dyn RuntimeBackend>>,
) -> Result<(), BackendError> {
    backend.remove_container(&name, force).await
}

/// Returns true if the runtime is reachable.
///
/// Frontend: `await invoke('ping')`
#[tauri::command]
pub async fn ping(
    backend: State<'_, Arc<dyn RuntimeBackend>>,
) -> Result<bool, BackendError> {
    Ok(backend.ping().await)
}
