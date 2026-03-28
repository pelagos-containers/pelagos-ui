//! # pelagos-protocol
//!
//! Shared types for the pelagos vsock IPC protocol.
//!
//! ## Overview
//!
//! Communication between the pelagos-ui host process and the pelagos-guest
//! daemon inside the Linux VM is newline-delimited JSON over a vsock socket.
//!
//! ```text
//! host → guest:  one JSON line per request  (GuestCommand)
//! guest → host:  one or more JSON lines      (GuestResponse)
//! ```
//!
//! Every exchange follows this pattern:
//! 1. Host sends a single [`GuestCommand`] JSON line terminated by `\n`.
//! 2. Guest sends zero or more [`GuestResponse::Stream`] lines (stdout/stderr).
//! 3. Guest sends exactly one terminal response: [`GuestResponse::Exit`],
//!    [`GuestResponse::Pong`], [`GuestResponse::ContainerList`], or
//!    [`GuestResponse::Error`].
//!
//! The [`GuestResponse::RawBytes`] variant is a special case: the guest emits
//! this JSON line followed immediately by exactly `size` raw bytes (no JSON
//! framing), then an [`GuestResponse::Exit`] line.
//!
//! ## Protocol versioning
//!
//! The [`GuestCommand::Version`] exchange allows the host to negotiate
//! compatibility.  The guest replies with [`GuestResponse::VersionInfo`]
//! containing [`PROTOCOL_VERSION`].  Hosts should reject connections where
//! `guest_version > host_version` (the guest is newer and may send variants
//! the host does not know).
//!
//! ## Wire format invariants (must never change)
//!
//! - Every line is valid UTF-8 JSON terminated by exactly one `\n`.
//! - [`GuestCommand`] uses `#[serde(tag = "cmd", rename_all = "snake_case")]`.
//! - [`GuestResponse`] uses `#[serde(tag = "type", rename_all = "snake_case")]`.
//! - Unknown `type` values in [`GuestResponse`] must be silently skipped by
//!   the host (forward-compatible parsing).
//! - Unknown fields in any struct must be silently ignored (`#[serde(deny_unknown_fields)]`
//!   is intentionally absent).

pub mod command;
pub mod response;
pub mod types;

pub use command::GuestCommand;
pub use response::GuestResponse;
pub use types::{ContainerInfo, ContainerStatus, GuestMount, HealthStatus, ImageInfo, VmStatus};

/// Current protocol version.  Increment when adding new variants or fields
/// that older guests cannot handle.  Both sides embed this in their binary.
pub const PROTOCOL_VERSION: u32 = 2;
