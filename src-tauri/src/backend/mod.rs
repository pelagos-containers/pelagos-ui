//! Platform abstraction for communicating with the pelagos runtime.
//!
//! On Linux the backend spawns `pelagos` CLI subprocesses directly.
//! On macOS it connects to `pelagos-guest` inside the VM over vsock.
//! The Svelte frontend and Tauri commands never know which is active.

#[cfg(target_os = "linux")]
pub mod process;

#[cfg(target_os = "macos")]
pub mod vsock;

use pelagos_protocol::ContainerInfo;
use tokio::sync::mpsc::UnboundedSender;

/// Errors returned by backend operations.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[cfg(target_os = "linux")]
    #[error("pelagos binary not found in PATH")]
    BinaryNotFound,
    #[cfg(target_os = "linux")]
    #[error("pelagos exited with status {code}: {stderr}")]
    CommandFailed { code: i32, stderr: String },
    #[error("failed to parse pelagos output: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[cfg(target_os = "macos")]
    #[error("{0}")]
    Other(String),
}

// Tauri requires command errors to be serialisable so they can cross the IPC boundary.
impl serde::Serialize for BackendError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// The runtime operations the UI needs.
///
/// All methods are async so both the subprocess backend (tokio::process) and
/// the vsock backend (tokio net) implement them with the same interface.
#[async_trait::async_trait]
pub trait RuntimeBackend: Send + Sync + 'static {
    /// List all containers (running + exited).
    async fn list_containers(&self) -> Result<Vec<ContainerInfo>, BackendError>;

    /// Stop a running container by name (SIGTERM).
    async fn stop_container(&self, name: &str) -> Result<(), BackendError>;

    /// Remove a container by name.  `force` stops it first if still running.
    async fn remove_container(&self, name: &str, force: bool) -> Result<(), BackendError>;

    /// Returns true if the runtime is reachable.
    /// Linux: pelagos binary responds to --version.
    /// macOS: guest daemon responds to ping over vsock.
    async fn ping(&self) -> bool;

    /// Start a container from `image`.  Each line of stdout/stderr is sent to
    /// `tx` as it arrives.  Returns the container process exit code (0 = success,
    /// or the container name when `detach` is true and the guest prints it).
    async fn run_container(
        &self,
        image: &str,
        name: Option<&str>,
        args: Vec<String>,
        detach: bool,
        tx: UnboundedSender<String>,
    ) -> Result<i32, BackendError>;
}
