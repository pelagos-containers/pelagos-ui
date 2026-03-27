//! macOS backend: communicates with pelagos-guest over vsock (Unix domain socket).
//!
//! The pelagos-mac daemon exposes the guest's vsock port as a Unix socket at:
//!   ~/.local/share/pelagos/vm.sock  (default profile)
//!
//! Wire protocol: newline-delimited JSON.  See pelagos-protocol crate.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{BackendError, RuntimeBackend};
use pelagos_protocol::{response::StreamKind, ContainerInfo, GuestCommand, GuestResponse};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;

pub fn default_socket_path() -> PathBuf {
    // pelagos-mac daemon socket: ~/.local/share/pelagos/vm.sock
    // This matches StateDir::open_profile("default") in pelagos-mac.
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".local/share/pelagos/vm.sock")
}

pub struct VsockBackend {
    socket_path: PathBuf,
}

impl VsockBackend {
    pub fn new(socket_path: impl AsRef<Path>) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_owned(),
        }
    }

    pub fn with_default_path() -> Self {
        Self::new(default_socket_path())
    }

    /// Send one command over a fresh connection, collect all response lines
    /// until a terminal variant, return accumulated stdout.
    async fn roundtrip(&self, cmd: &GuestCommand) -> Result<String, BackendError> {
        let stream = timeout(
            Duration::from_secs(5),
            UnixStream::connect(&self.socket_path),
        )
        .await
        .map_err(|_| BackendError::Other("vsock connect timed out".into()))?
        .map_err(BackendError::Io)?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let mut line = serde_json::to_string(cmd).map_err(BackendError::ParseError)?;
        line.push('\n');
        writer
            .write_all(line.as_bytes())
            .await
            .map_err(BackendError::Io)?;

        let mut stdout = String::new();
        let mut buf = String::new();
        loop {
            buf.clear();
            let n = reader.read_line(&mut buf).await.map_err(BackendError::Io)?;
            if n == 0 {
                break;
            }
            match serde_json::from_str::<GuestResponse>(buf.trim()) {
                Ok(GuestResponse::Stream {
                    stream: StreamKind::Stdout,
                    data,
                }) => {
                    stdout.push_str(&data);
                }
                Ok(GuestResponse::Exit { .. }) | Ok(GuestResponse::Pong { .. }) => break,
                Ok(GuestResponse::Error { error }) => {
                    return Err(BackendError::Other(error));
                }
                Ok(_) => {} // forward-compat: skip unknown variants
                Err(e) => log::warn!("unparseable guest response: {e}: {}", buf.trim()),
            }
        }
        Ok(stdout)
    }
}

#[async_trait::async_trait]
impl RuntimeBackend for VsockBackend {
    async fn list_containers(&self) -> Result<Vec<ContainerInfo>, BackendError> {
        let cmd = GuestCommand::Ps {
            all: true,
            json: true,
        };
        let stdout = self.roundtrip(&cmd).await?;
        Ok(serde_json::from_str(&stdout)?)
    }

    async fn stop_container(&self, name: &str) -> Result<(), BackendError> {
        self.roundtrip(&GuestCommand::Stop {
            name: name.to_string(),
        })
        .await?;
        Ok(())
    }

    async fn remove_container(&self, name: &str, force: bool) -> Result<(), BackendError> {
        self.roundtrip(&GuestCommand::Rm {
            name: name.to_string(),
            force,
        })
        .await?;
        Ok(())
    }

    async fn ping(&self) -> bool {
        self.roundtrip(&GuestCommand::Ping).await.is_ok()
    }
}
