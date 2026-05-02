//! Responses sent from the guest daemon (pelagos-guest) to the host (pelagos-ui).
//!
//! Every response is a `\n`-terminated JSON line.  Hosts must skip (not error on)
//! any `type` value they do not recognise — this is the forward-compatibility rule.

use serde::{Deserialize, Serialize};

/// A response line sent from the guest daemon to the UI host over vsock.
///
/// ## Parsing advice
///
/// Deserialise each line independently.  Never accumulate multiple lines and
/// try to parse them as one JSON value.  The terminal variants ([`GuestResponse::Exit`],
/// [`GuestResponse::Pong`], [`GuestResponse::Error`], etc.) signal end-of-response.
///
/// ```rust,ignore
/// let mut stdout_buf = String::new();
/// loop {
///     let line = read_line(&mut reader)?;
///     match serde_json::from_str::<GuestResponse>(&line)? {
///         GuestResponse::Stream { stream: StreamKind::Stdout, data } => {
///             stdout_buf.push_str(&data);
///         }
///         GuestResponse::Exit { exit } => {
///             // command finished; exit code in `exit`
///             break;
///         }
///         GuestResponse::Error { error } => {
///             return Err(error.into());
///         }
///         _ => {} // ignore unknown / non-stdout variants
///     }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuestResponse {
    // ------------------------------------------------------------------
    // Streaming output
    // ------------------------------------------------------------------
    /// A chunk of stdout or stderr from the running command.
    /// Data is UTF-8 text (not binary).  Lines may be split across chunks.
    Stream {
        stream: StreamKind,
        /// The text content of this chunk (may be multi-line).
        data: String,
    },

    // ------------------------------------------------------------------
    // Terminal responses (exactly one per command)
    // ------------------------------------------------------------------
    /// The command exited.  `exit` is the process exit code (0 = success).
    /// Always the final response for commands that spawn a process.
    Exit { exit: i32 },

    /// Response to [`crate::command::GuestCommand::Ping`].
    Pong { pong: bool },

    /// Response to [`crate::command::GuestCommand::Version`].
    VersionInfo {
        /// Protocol version implemented by this guest binary.
        /// Compare with [`crate::PROTOCOL_VERSION`] in the host.
        version: u32,
        /// Human-readable guest binary version string (semver).
        pelagos_version: String,
    },

    /// An error occurred before any output was produced.
    /// `error` is a human-readable message.  When this is emitted, no
    /// [`GuestResponse::Exit`] follows — `Error` is itself terminal.
    Error { error: String },

    // ------------------------------------------------------------------
    // Special: raw binary payload
    // ------------------------------------------------------------------
    /// Precedes a raw binary payload of `size` bytes written directly to the
    /// socket (no JSON framing).  After the raw bytes, the guest sends
    /// [`GuestResponse::Exit`].
    ///
    /// Used by `CpFrom`: the host should read exactly `size` bytes from the
    /// socket after receiving this line, then expect the `Exit` line.
    RawBytes { size: u64 },

    // ------------------------------------------------------------------
    // Readiness / informational
    // ------------------------------------------------------------------
    /// Emitted by the guest when it is ready to accept the next command on
    /// a multiplexed connection.  Hosts may ignore this.
    Ready { ready: bool },

    // ------------------------------------------------------------------
    // Kubernetes (rusternetes)
    // ------------------------------------------------------------------
    /// Response to [`crate::command::GuestCommand::KubernetesStatus`].
    /// `running` is `true` when both api-server and kubelet are running.
    KubernetesStatus { running: bool },
}

/// Which output stream a [`GuestResponse::Stream`] chunk came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Stdout,
    Stderr,
}
