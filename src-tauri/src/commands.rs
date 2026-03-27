//! Tauri commands exposed to the Svelte frontend via `invoke()`.
//!
//! Each command is a thin async wrapper over RuntimeBackend.
//! Errors serialise as strings (BackendError implements Serialize).

use std::sync::Arc;
use tauri::{Emitter, State};

use crate::backend::{BackendError, RuntimeBackend};
use pelagos_protocol::{ContainerInfo, VmStatus};

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

/// Start a container.  Streams stdout/stderr as `run-log` Tauri events.
/// Returns the exit code (0 = success).
///
/// Frontend: subscribe to `run-log` before calling, then
/// `await invoke('run_container', { image, name, args, detach })`
#[tauri::command]
pub async fn run_container(
    app: tauri::AppHandle,
    backend: State<'_, Arc<dyn RuntimeBackend>>,
    image: String,
    name: Option<String>,
    args: Vec<String>,
    detach: bool,
) -> Result<i32, BackendError> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let app2 = app.clone();
    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            let _ = app2.emit("run-log", line);
        }
    });
    backend
        .run_container(&image, name.as_deref(), args, detach, tx)
        .await
}

/// Open a new terminal window running `pelagos run --tty --interactive image [cmd]`.
/// Returns immediately — the terminal handles the session.
///
/// Frontend: `await invoke('launch_interactive', { image, name, args })`
#[tauri::command]
pub fn launch_interactive(
    image: String,
    name: Option<String>,
    args: Vec<String>,
) -> Result<(), String> {
    crate::terminal::open_in_terminal(&image, name.as_deref(), &args)
}

/// Returns true if the runtime is reachable.
///
/// Frontend: `await invoke('ping')`
#[tauri::command]
pub async fn ping(backend: State<'_, Arc<dyn RuntimeBackend>>) -> Result<bool, BackendError> {
    Ok(backend.ping().await)
}

/// Returns the VM status: Running if the guest daemon responds to ping, Stopped otherwise.
///
/// Frontend: `await invoke('vm_status')` → `"Running"` | `"Stopped"` | `"Starting"`
#[tauri::command]
pub async fn vm_status(
    backend: State<'_, Arc<dyn RuntimeBackend>>,
) -> Result<VmStatus, BackendError> {
    if backend.ping().await {
        Ok(VmStatus::Running)
    } else {
        Ok(VmStatus::Stopped)
    }
}
