//! Shared data types used in both commands and responses.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// VM-level types
// ---------------------------------------------------------------------------

/// Whether the guest VM is reachable and running.
///
/// Produced by the host-side backend, not the guest (the guest is obviously
/// running if it responded).  The host infers VM state from connection
/// success/failure and, on macOS, from the AVF VM lifecycle callbacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VmStatus {
    /// VM is running and the guest daemon responded to a ping within the timeout.
    Running,
    /// VM process exists but the guest daemon did not respond.
    Starting,
    /// VM is not running.
    Stopped,
}

// ---------------------------------------------------------------------------
// Container-level types
// ---------------------------------------------------------------------------

/// Container lifecycle status.
///
/// Serialises as lowercase strings: `"running"` / `"exited"`.
/// This matches the serialisation in `pelagos/src/cli/mod.rs` so that
/// `pelagos ps --format json` output can be deserialised directly into this type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerStatus {
    Running,
    Exited,
}

impl std::fmt::Display for ContainerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerStatus::Running => write!(f, "running"),
            ContainerStatus::Exited => write!(f, "exited"),
        }
    }
}

/// Health check status for containers that declare a HEALTHCHECK.
///
/// Serialises as lowercase strings: `"starting"` / `"healthy"` / `"unhealthy"` / `"none"`.
/// Matches `pelagos/src/cli/mod.rs` HealthStatus serialisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Starting,
    Healthy,
    Unhealthy,
    None,
}

/// A virtiofs bind mount applied inside a container.
///
/// Used in [`crate::command::GuestCommand::Run`] to describe which virtiofs share
/// to bind into the container and at what path.
///
/// The pelagos-mac daemon pre-mounts virtiofs shares in the VM at `/mnt/<tag>`:
/// - `share0` is always `$HOME` (available without VM restart).
/// - Additional shares for paths outside `$HOME` require a VM restart.
///
/// The guest daemon constructs the bind-mount path as:
/// - `subpath` empty → `/mnt/<tag>/<container_path>`
/// - `subpath` non-empty → `/mnt/<tag>/<subpath>` → `<container_path>`
///
/// ## Wire format
///
/// ```json
/// {"tag":"share0","subpath":"mysite","container_path":"/usr/share/nginx/html"}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestMount {
    /// virtiofs tag — the host directory is mounted at `/mnt/<tag>` in the VM.
    /// `share0` is always `$HOME`.
    pub tag: String,
    /// Relative subpath within the virtiofs mount root (empty = mount the whole share).
    #[serde(default)]
    pub subpath: String,
    /// Absolute path inside the container where the directory appears.
    pub container_path: String,
    /// Mount is read-only if `true`.
    #[serde(default)]
    pub read_only: bool,
}

/// A minimal, stable view of a running or exited container.
///
/// This is a **projection** of `pelagos/src/cli/mod.rs::ContainerState` — it
/// contains only the fields the UI needs.  It is intentionally narrower so
/// that additions to `ContainerState` upstream do not require protocol bumps.
///
/// ## Deserialisation from `pelagos ps --format json`
///
/// `pelagos ps --format json` outputs a JSON array of `ContainerState` objects.
/// Because `ContainerInfo` is a strict subset of those fields and serde ignores
/// unknown fields by default, a `Vec<ContainerInfo>` can be deserialised
/// directly from that output without any transformation:
///
/// ```rust,ignore
/// let output = std::process::Command::new("pelagos")
///     .args(["ps", "--all", "--format", "json"])
///     .output()?;
/// let containers: Vec<ContainerInfo> = serde_json::from_slice(&output.stdout)?;
/// ```
///
/// ## JSON shape (stable contract)
///
/// ```json
/// {
///   "name":       "my-app",
///   "status":     "running",
///   "pid":        12345,
///   "started_at": "2026-03-15T12:00:00Z",
///   "rootfs":     "ubuntu:22.04",
///   "command":    ["/bin/sh", "-c", "sleep 1000"],
///   "image":      "ubuntu:22.04",          // optional; absent for rootfs containers
///   "exit_code":  null,                    // null while running; integer when exited
///   "health":     "healthy",               // optional; absent if no HEALTHCHECK
///   "bridge_ip":  "172.19.0.5",            // optional; absent if no bridge network
///   "network_ips": { "frontend": "10.0.0.2" }, // optional; empty if not present
///   "labels":     { "env": "staging" }     // optional; empty if not present
/// }
/// ```
///
/// Fields marked optional may be absent in the JSON (they use `skip_serializing_if`
/// in pelagos).  All such fields deserialise to `None` / empty collection here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    /// Unique container name (user-assigned or auto-generated).
    pub name: String,

    /// Current lifecycle status.
    pub status: ContainerStatus,

    /// PID of the container process inside the VM.  0 if not yet assigned
    /// (brief window during detached startup).
    pub pid: i32,

    /// ISO 8601 timestamp of container start (e.g. `"2026-03-15T12:00:00Z"`).
    pub started_at: String,

    /// Rootfs path or image reference used at container start.
    /// For OCI image containers this is the registry ref (e.g. `"alpine:3.19"`).
    /// For bind-rootfs containers this is the path (e.g. `"/var/lib/pelagos/rootfs/alpine"`).
    pub rootfs: String,

    /// Full command vector run inside the container.
    pub command: Vec<String>,

    /// OCI image reference, present when the container was started from a pulled image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,

    /// Exit code.  `None` while the container is running; set when `status == exited`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,

    /// Health check state.  `None` if no HEALTHCHECK was defined for this container.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<HealthStatus>,

    /// IP address on the default bridge network.  `None` for non-bridge containers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge_ip: Option<String>,

    /// IP addresses per named network.  Empty if not using named networks.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub network_ips: HashMap<String, String>,

    /// Container labels.  Empty if none were assigned.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, String>,
}

/// A locally cached OCI image.
///
/// Deserialised from `pelagos image ls --json` output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    /// Full OCI reference, e.g. `"docker.io/library/alpine:latest"`.
    pub reference: String,
    /// Content-addressable digest, e.g. `"sha256:abc123..."`.
    pub digest: String,
    /// Layer digests (may be empty for single-layer images).
    #[serde(default)]
    pub layers: Vec<String>,
}

impl ImageInfo {
    /// First 12 hex chars of the digest, without the `"sha256:"` prefix.
    pub fn short_digest(&self) -> &str {
        let hex = self.digest.strip_prefix("sha256:").unwrap_or(&self.digest);
        &hex[..hex.len().min(12)]
    }
}

impl ContainerInfo {
    /// Returns `true` if the container is currently running.
    pub fn is_running(&self) -> bool {
        self.status == ContainerStatus::Running
    }

    /// Returns a short human-readable summary of the command (truncated to `max` chars).
    pub fn command_summary(&self, max: usize) -> String {
        let s = self.command.join(" ");
        if s.len() > max {
            format!("{}…", &s[..max.saturating_sub(1)])
        } else {
            s
        }
    }
}
