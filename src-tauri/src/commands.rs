//! Tauri commands exposed to the Svelte frontend via `invoke()`.
//!
//! Each command is a thin async wrapper over RuntimeBackend.
//! Errors serialise as strings (BackendError implements Serialize).

use std::sync::Arc;
use tauri::{Emitter, State};

use crate::backend::{BackendError, RuntimeBackend};
use pelagos_protocol::{ContainerInfo, GuestMount, ImageInfo, VmStatus};

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

/// Open a new terminal window running `pelagos run --tty --interactive image [cmd]`.
/// Returns immediately — the terminal handles the session.
///
/// `volumes` is a list of `HOST:CONTAINER[:ro]` strings passed as `-v` flags
/// to the pelagos-mac CLI, which handles the virtiofs translation itself.
///
/// Frontend: `await invoke('launch_interactive', { image, name, args, ports, volumes })`
#[tauri::command]
pub fn launch_interactive(
    image: String,
    name: Option<String>,
    args: Vec<String>,
    ports: Vec<String>,
    volumes: Vec<String>,
) -> Result<(), String> {
    crate::terminal::open_in_terminal(&image, name.as_deref(), &args, &ports, &volumes)
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
