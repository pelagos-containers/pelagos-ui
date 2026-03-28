//! Commands sent from the host (pelagos-ui) to the guest daemon (pelagos-guest).
//!
//! Each variant serialises to a JSON object with a `"cmd"` discriminant field,
//! e.g. `{"cmd":"ping"}` or `{"cmd":"ps","all":true,"json":true}`.
//!
//! ## Compatibility rule
//!
//! Guests running an older protocol version may not recognise newer `cmd` values.
//! Always send [`GuestCommand::Version`] first and check the response before using
//! any variant added after protocol version 1.

use serde::{Deserialize, Serialize};

use crate::types::GuestMount;

/// A command sent from the UI host process to the guest daemon over vsock.
///
/// Serialised as a single `\n`-terminated JSON line.
///
/// ## Wire examples
///
/// ```json
/// {"cmd":"ping"}
/// {"cmd":"version"}
/// {"cmd":"ps","all":true,"json":true}
/// {"cmd":"stop","name":"my-app"}
/// {"cmd":"rm","name":"my-app","force":false}
/// {"cmd":"run","image":"alpine:3.19","args":["/bin/sh"],"detach":false}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum GuestCommand {
    // ------------------------------------------------------------------
    // Lifecycle / meta
    // ------------------------------------------------------------------
    /// Health check.  Guest replies with [`crate::response::GuestResponse::Pong`].
    /// Round-trip latency should be < 5 ms over vsock.
    Ping,

    /// Request the guest's protocol version.
    /// Guest replies with [`crate::response::GuestResponse::VersionInfo`].
    /// Always send this before using commands added after version 1.
    Version,

    // ------------------------------------------------------------------
    // Container introspection (UI-primary)
    // ------------------------------------------------------------------
    /// List containers.  Maps to `pelagos ps [--all] [--format json]`.
    ///
    /// When `json` is `true`, the guest passes `--format json` to the pelagos
    /// binary and the caller receives streamed JSON output that deserialises as
    /// `Vec<ContainerInfo>`.  When `json` is `false`, output is a human-readable
    /// table (the legacy behaviour, preserved for `pelagos-mac` CLI compatibility).
    ///
    /// The UI backend **always** sets `json: true`.
    ///
    /// Guest replies with one or more [`crate::response::GuestResponse::Stream`]
    /// lines followed by [`crate::response::GuestResponse::Exit`].  When
    /// `json == true`, accumulate all stdout stream data and deserialise as
    /// `Vec<ContainerInfo>` when `Exit` arrives.
    Ps {
        /// Include exited containers (equivalent to `pelagos ps --all`).
        #[serde(default)]
        all: bool,
        /// Request JSON output (`--format json`). Always `true` for UI callers.
        #[serde(default)]
        json: bool,
    },

    // ------------------------------------------------------------------
    // Container lifecycle
    // ------------------------------------------------------------------
    /// Start a container from an image.  Maps to `pelagos run`.
    ///
    /// Guest replies with streamed stdout/stderr followed by
    /// [`crate::response::GuestResponse::Exit`].
    Run {
        /// OCI image reference (e.g. `"alpine:3.19"`, `"ghcr.io/org/app:latest"`).
        image: String,
        /// Arguments passed after the image name (the command to run inside the container).
        #[serde(default)]
        args: Vec<String>,
        /// Environment variables as `KEY=VALUE` strings.
        #[serde(default)]
        env: std::collections::HashMap<String, String>,
        /// Virtiofs bind mounts (tag + subpath → container path).
        #[serde(default)]
        mounts: Vec<GuestMount>,
        /// Optional container name (`--name`).
        #[serde(default)]
        name: Option<String>,
        /// Run detached (`--detach`).
        #[serde(default)]
        detach: bool,
        /// Port mappings `HOST:CONTAINER` forwarded to `pelagos run --publish`.
        #[serde(default)]
        publish: Vec<String>,
        /// Network mode forwarded to `pelagos run --network` (e.g. `"pasta"`).
        #[serde(default)]
        network: Option<String>,
    },

    /// Stop a running container by name.  Maps to `pelagos stop <name>`.
    ///
    /// Guest replies with [`crate::response::GuestResponse::Exit`].
    Stop { name: String },

    /// Remove a container by name.  Maps to `pelagos rm [--force] <name>`.
    ///
    /// Guest replies with [`crate::response::GuestResponse::Exit`].
    Rm {
        name: String,
        /// Remove even if still running (`--force`).
        #[serde(default)]
        force: bool,
    },

    // ------------------------------------------------------------------
    // Exec / interactive
    // ------------------------------------------------------------------
    /// Run a command inside a running container (namespace join).
    /// Maps to `pelagos exec [--tty] <container> <args...>`.
    ExecInto {
        /// Name of the running container to exec into.
        container: String,
        /// Command + arguments to run inside the container.
        args: Vec<String>,
        #[serde(default)]
        env: std::collections::HashMap<String, String>,
        /// Allocate a PTY.
        #[serde(default)]
        tty: bool,
    },

    // ------------------------------------------------------------------
    // Logs
    // ------------------------------------------------------------------
    /// Stream container logs.  Maps to `pelagos logs [--follow] <name>`.
    ///
    /// When `follow` is `true`, the guest streams indefinitely until the host
    /// closes the connection.
    Logs {
        name: String,
        #[serde(default)]
        follow: bool,
    },

    // ------------------------------------------------------------------
    // VM shell (debug / development)
    // ------------------------------------------------------------------
    /// Open a shell directly in the VM (no container, no namespaces).
    /// Not intended for production UI use.
    Shell {
        #[serde(default)]
        tty: bool,
    },

    // ------------------------------------------------------------------
    // Image management
    // ------------------------------------------------------------------
    /// List locally cached OCI images.  Maps to `pelagos image ls [--json]`.
    ///
    /// When `json` is `true`, streams JSON output.  The UI backend always
    /// sets `json: true` and deserialises accumulated stdout as `Vec<ImageInfo>`.
    ImageLs {
        #[serde(default)]
        json: bool,
    },

    /// Pull an OCI image from a registry.  Maps to `pelagos image pull <reference>`.
    ///
    /// Guest streams stdout/stderr progress lines and ends with `Exit`.
    ImagePull { reference: String },

    /// Remove a locally cached OCI image.  Maps to `pelagos image rm <reference>`.
    ///
    /// Guest replies with `Exit` (exit code 0 = success).
    ImageRm { reference: String },

    /// Proxy a `pelagos compose` subcommand to the VM guest.
    ///
    /// The compose file and all relative bind-mount paths must be accessible
    /// inside the VM.  On macOS, `$HOME` is always mounted as virtiofs `share0`
    /// at `/mnt/share0`; the macOS CLI translates host paths before sending
    /// this command.  `working_dir` is the parent directory of `file` in the VM
    /// so that relative paths inside the compose file resolve correctly.
    ///
    /// Guest replies with streamed stdout/stderr followed by
    /// [`crate::response::GuestResponse::Exit`].  For `up` (daemonised), the
    /// stream ends quickly.  For `up --foreground` or `logs --follow`, the
    /// stream continues until the client disconnects or the stack stops.
    ///
    /// ## Wire example
    ///
    /// ```json
    /// {"cmd":"compose","subcommand":"up","file":"/mnt/share0/Projects/foo/compose.reml","working_dir":"/mnt/share0/Projects/foo"}
    /// ```
    Compose {
        /// Compose subcommand: `"up"`, `"down"`, `"ps"`, or `"logs"`.
        subcommand: String,
        /// Absolute path to the `.reml` file inside the VM.
        file: String,
        /// Working directory for the invocation (typically the directory containing `file`).
        working_dir: String,
        /// Optional project name override (`-p`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
        /// Extra flags forwarded verbatim (e.g. `["--foreground"]`, `["--follow", "grafana"]`).
        #[serde(default)]
        args: Vec<String>,
    },
}
