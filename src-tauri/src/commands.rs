//! Tauri commands exposed to the Svelte frontend via `invoke()`.
//!
//! Each command is a thin async wrapper over RuntimeBackend.
//! Errors serialise as strings (BackendError implements Serialize).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, State};

use crate::backend::{BackendError, RuntimeBackend};
use pelagos_protocol::{ContainerInfo, GuestMount, ImageInfo, VmStatus};

/// Managed state tracking active log-streaming tasks, keyed by container name.
pub struct LogState(pub Mutex<HashMap<String, tauri::async_runtime::JoinHandle<()>>>);

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
/// `volumes` is a list of `HOST_PATH:CONTAINER_PATH[:ro]` strings.
/// Host paths must be under `$HOME` — the pelagos-mac daemon always shares
/// `$HOME` as virtiofs tag `share0`, so those paths are reachable in the VM
/// without a restart.  Paths outside `$HOME` are rejected with an error.
///
/// Frontend: subscribe to `run-log` before calling, then
/// `await invoke('run_container', { image, name, args, detach, ports, volumes })`
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn run_container(
    app: tauri::AppHandle,
    backend: State<'_, Arc<dyn RuntimeBackend>>,
    image: String,
    name: Option<String>,
    args: Vec<String>,
    detach: bool,
    ports: Vec<String>,
    volumes: Vec<String>,
) -> Result<i32, BackendError> {
    let mounts = parse_volumes(&volumes)?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let app2 = app.clone();
    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            let _ = app2.emit("run-log", line);
        }
    });
    backend
        .run_container(&image, name.as_deref(), args, detach, ports, mounts, tx)
        .await
}

/// Parse a list of `HOST:CONTAINER[:ro]` volume specs into [`GuestMount`]s.
///
/// Host paths must be absolute and under `$HOME`.  `$HOME` is always shared
/// with the VM as virtiofs tag `share0`; subdirectories within it are available
/// as subpaths of that share without requiring a VM restart.
///
/// Returns an error for any spec that is malformed or outside `$HOME`.
#[cfg(target_os = "macos")]
fn parse_volumes(specs: &[String]) -> Result<Vec<GuestMount>, BackendError> {
    if specs.is_empty() {
        return Ok(vec![]);
    }
    let home =
        dirs::home_dir().ok_or_else(|| BackendError::Other("cannot determine $HOME".into()))?;
    let mut mounts = Vec::with_capacity(specs.len());
    for spec in specs {
        let parts: Vec<&str> = spec.splitn(3, ':').collect();
        if parts.len() < 2 {
            return Err(BackendError::Other(format!(
                "invalid volume spec {spec:?}: expected HOST:CONTAINER or HOST:CONTAINER:ro"
            )));
        }
        let raw = parts[0];
        let expanded = if let Some(rest) = raw.strip_prefix("~/") {
            home.join(rest)
        } else if raw == "~" {
            home.clone()
        } else {
            std::path::PathBuf::from(raw)
        };
        let host_path = expanded.as_path();
        let container_path = parts[1].to_string();
        let read_only = parts.get(2).map(|s| *s == "ro").unwrap_or(false);
        if !host_path.is_absolute() {
            return Err(BackendError::Other(format!(
                "volume host path {raw:?} must be absolute (or start with ~/)"
            )));
        }
        let subpath = host_path.strip_prefix(&home).map_err(|_| {
            BackendError::Other(format!(
                "volume host path {} is outside $HOME — only paths under $HOME \
                 are accessible in the VM without a restart",
                host_path.display()
            ))
        })?;
        mounts.push(GuestMount {
            tag: "share0".into(),
            subpath: subpath.to_string_lossy().into_owned(),
            container_path,
            read_only,
        });
    }
    Ok(mounts)
}

#[cfg(not(target_os = "macos"))]
fn parse_volumes(specs: &[String]) -> Result<Vec<GuestMount>, BackendError> {
    // On Linux the process backend passes mounts directly as bind-mounts.
    // GuestMount.subpath is unused; tag and subpath are repurposed as src path.
    Ok(specs
        .iter()
        .filter_map(|spec| {
            let parts: Vec<&str> = spec.splitn(3, ':').collect();
            if parts.len() < 2 {
                return None;
            }
            let read_only = parts.get(2).map(|s| *s == "ro").unwrap_or(false);
            Some(GuestMount {
                tag: parts[0].to_string(),
                subpath: String::new(),
                container_path: parts[1].to_string(),
                read_only,
            })
        })
        .collect())
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

/// List locally cached OCI images.
///
/// Frontend: `await invoke('list_images')`
#[tauri::command]
pub async fn list_images(
    backend: State<'_, Arc<dyn RuntimeBackend>>,
) -> Result<Vec<ImageInfo>, BackendError> {
    backend.list_images().await
}

/// Pull an OCI image from a registry.  Streams progress as `pull-log` Tauri events.
/// Returns the exit code (0 = success).
///
/// Frontend: subscribe to `pull-log` then `await invoke('pull_image', { reference })`
#[tauri::command]
pub async fn pull_image(
    app: tauri::AppHandle,
    backend: State<'_, Arc<dyn RuntimeBackend>>,
    reference: String,
) -> Result<i32, BackendError> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let app2 = app.clone();
    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            let _ = app2.emit("pull-log", line);
        }
    });
    backend.pull_image(&reference, tx).await
}

/// Remove a locally cached OCI image.
///
/// Frontend: `await invoke('remove_image', { reference })`
#[tauri::command]
pub async fn remove_image(
    reference: String,
    backend: State<'_, Arc<dyn RuntimeBackend>>,
) -> Result<(), BackendError> {
    backend.remove_image(&reference).await
}

/// Start streaming logs for a container.  Returns immediately; log lines
/// arrive as `log-line` events: `{ name: string, line: string }`.
/// Calling again for the same container cancels the previous stream.
///
/// Frontend: subscribe to `log-line`, then `await invoke('stream_logs', { name, follow })`
#[tauri::command]
pub async fn stream_logs(
    app: tauri::AppHandle,
    backend: State<'_, Arc<dyn RuntimeBackend>>,
    log_state: State<'_, LogState>,
    name: String,
    follow: bool,
) -> Result<(), BackendError> {
    if let Some(h) = log_state.0.lock().unwrap().remove(&name) {
        h.abort();
    }

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let backend_arc = backend.inner().clone();
    let name_stream = name.clone();
    let name_emit = name.clone();
    let app2 = app.clone();

    let handle = tauri::async_runtime::spawn(async move {
        let emitter = tauri::async_runtime::spawn(async move {
            while let Some(line) = rx.recv().await {
                let _ = app2.emit(
                    "log-line",
                    serde_json::json!({ "name": name_emit, "line": line }),
                );
            }
        });
        let _ = backend_arc.stream_logs(&name_stream, follow, tx).await;
        emitter.abort();
    });

    log_state.0.lock().unwrap().insert(name, handle);
    Ok(())
}

/// Stop an active log stream for a container.
///
/// Frontend: `await invoke('stop_logs', { name })`
#[tauri::command]
pub fn stop_logs(log_state: State<'_, LogState>, name: String) {
    if let Some(h) = log_state.0.lock().unwrap().remove(&name) {
        h.abort();
    }
}

/// Return true if rusternetes (api-server + kubelet) is running.
///
/// Frontend: `await invoke('kubernetes_status')`
#[tauri::command]
pub async fn kubernetes_status(
    backend: State<'_, Arc<dyn RuntimeBackend>>,
) -> Result<bool, BackendError> {
    backend.kubernetes_status().await
}

/// Start the rusternetes control plane.  Progress lines are emitted as
/// `kubernetes-start-log` events.
///
/// Frontend: `await invoke('start_kubernetes')`
#[tauri::command]
pub async fn start_kubernetes(
    app: tauri::AppHandle,
    backend: State<'_, Arc<dyn RuntimeBackend>>,
) -> Result<(), BackendError> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let backend_arc = backend.inner().clone();
    tauri::async_runtime::spawn(async move {
        while let Some(line) = rx.recv().await {
            let _ = app.emit("kubernetes-start-log", line);
        }
    });
    backend_arc.start_kubernetes(tx).await
}

/// Stop the rusternetes control plane.
///
/// Frontend: `await invoke('stop_kubernetes')`
#[tauri::command]
pub async fn stop_kubernetes(
    backend: State<'_, Arc<dyn RuntimeBackend>>,
) -> Result<(), BackendError> {
    backend.stop_kubernetes().await
}
