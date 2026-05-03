//! macOS backend: communicates with pelagos-guest over vsock (Unix domain socket).
//!
//! The pelagos-mac daemon exposes the guest's vsock port as a Unix socket at:
//!   ~/.local/share/pelagos/vm.sock  (default profile)
//!
//! Wire protocol: newline-delimited JSON.  See pelagos-protocol crate.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{BackendError, RuntimeBackend};
use pelagos_protocol::{
    response::StreamKind, ContainerInfo, GuestCommand, GuestMount, GuestResponse, ImageInfo,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::timeout;

/// Parse a `HOST:CONTAINER` port-forward spec into `(host_port, container_port)`.
/// Returns `None` if the spec is not in the expected format.
fn parse_port_spec(spec: &str) -> Option<(u16, u16)> {
    let (host_str, container_str) = spec.split_once(':')?;
    let host: u16 = host_str.parse().ok()?;
    let container: u16 = container_str.parse().ok()?;
    Some((host, container))
}

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

    /// Open a connection to the daemon socket.
    async fn connect(&self) -> Result<UnixStream, BackendError> {
        timeout(
            Duration::from_secs(5),
            UnixStream::connect(&self.socket_path),
        )
        .await
        .map_err(|_| BackendError::Other("vsock connect timed out".into()))?
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                BackendError::Other("VM stopped".into())
            } else {
                BackendError::Io(e)
            }
        })
    }

    /// Register a macOS-side port forward with the pelagos-mac daemon.
    ///
    /// Sends `DaemonCmd::RegisterPort` to the daemon socket.  The daemon
    /// handles this locally (no vsock trip) by binding `0.0.0.0:host_port`
    /// and relaying accepted connections through the NAT relay into the VM.
    /// The subscription watcher in the daemon auto-deregisters ports when the
    /// associated container exits.
    async fn register_port(&self, host_port: u16, container_port: u16) -> Result<(), BackendError> {
        let mut stream = self.connect().await?;
        let msg = format!(
            "{{\"RegisterPort\":{{\"host_port\":{host_port},\"container_port\":{container_port}}}}}\n"
        );
        stream
            .write_all(msg.as_bytes())
            .await
            .map_err(BackendError::Io)?;
        // Read the one-line DaemonResponse: {"status":"ok"} or {"status":"err",...}
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .map_err(BackendError::Io)?;
        let line = line.trim();
        if line.contains("\"err\"") {
            log::warn!("port {host_port}:{container_port} registration: {line}");
        }
        Ok(())
    }

    /// Send one command, collect all stdout, return it.
    async fn roundtrip(&self, cmd: &GuestCommand) -> Result<String, BackendError> {
        let stream = self.connect().await?;

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
        if stdout.trim().is_empty() {
            return Err(BackendError::Other("VM not ready".into()));
        }
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

    async fn run_container(
        &self,
        image: &str,
        name: Option<&str>,
        args: Vec<String>,
        detach: bool,
        ports: Vec<String>,
        mounts: Vec<GuestMount>,
        tx: UnboundedSender<String>,
    ) -> Result<i32, BackendError> {
        // Register macOS-side port listeners with the pelagos-mac daemon BEFORE
        // sending the guest run command.  Both are required for end-to-end forwarding:
        //   macOS:host_port → daemon PortDispatcher → NAT relay → VM:host_port
        //   VM:host_port ← pasta -t host_port:container_port ← container:container_port
        if !ports.is_empty() {
            for spec in &ports {
                if let Some((host, container)) = parse_port_spec(spec) {
                    let _ = self.register_port(host, container).await;
                }
            }
        }

        let network = if ports.is_empty() {
            None
        } else {
            Some("pasta".to_string())
        };
        let cmd = GuestCommand::Run {
            image: image.to_string(),
            args,
            name: name.map(str::to_string),
            detach,
            env: Default::default(),
            mounts,
            publish: ports,
            network,
        };

        let stream = self.connect().await?;
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let mut line = serde_json::to_string(&cmd).map_err(BackendError::ParseError)?;
        line.push('\n');
        writer
            .write_all(line.as_bytes())
            .await
            .map_err(BackendError::Io)?;

        let mut buf = String::new();
        let mut exit_code = 0;
        loop {
            buf.clear();
            let n = reader.read_line(&mut buf).await.map_err(BackendError::Io)?;
            if n == 0 {
                break;
            }
            match serde_json::from_str::<GuestResponse>(buf.trim()) {
                Ok(GuestResponse::Stream { data, .. }) => {
                    for line in data.lines() {
                        let _ = tx.send(line.to_string());
                    }
                }
                Ok(GuestResponse::Exit { exit }) => {
                    exit_code = exit;
                    break;
                }
                Ok(GuestResponse::Error { error }) => {
                    return Err(BackendError::Other(error));
                }
                Ok(_) => {}
                Err(e) => log::warn!("unparseable guest response: {e}: {}", buf.trim()),
            }
        }
        Ok(exit_code)
    }

    async fn list_images(&self) -> Result<Vec<ImageInfo>, BackendError> {
        let stdout = self
            .roundtrip(&GuestCommand::ImageLs { json: true })
            .await?;
        if stdout.trim().is_empty() {
            return Err(BackendError::Other("VM not ready".into()));
        }
        Ok(serde_json::from_str(&stdout)?)
    }

    async fn pull_image(
        &self,
        reference: &str,
        tx: UnboundedSender<String>,
    ) -> Result<i32, BackendError> {
        let cmd = GuestCommand::ImagePull {
            reference: reference.to_string(),
        };

        let stream = self.connect().await?;
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let mut line = serde_json::to_string(&cmd).map_err(BackendError::ParseError)?;
        line.push('\n');
        writer
            .write_all(line.as_bytes())
            .await
            .map_err(BackendError::Io)?;

        let mut buf = String::new();
        let mut exit_code = 0;
        loop {
            buf.clear();
            let n = reader.read_line(&mut buf).await.map_err(BackendError::Io)?;
            if n == 0 {
                break;
            }
            match serde_json::from_str::<GuestResponse>(buf.trim()) {
                Ok(GuestResponse::Stream { data, .. }) => {
                    for l in data.lines() {
                        let _ = tx.send(l.to_string());
                    }
                }
                Ok(GuestResponse::Exit { exit }) => {
                    exit_code = exit;
                    break;
                }
                Ok(GuestResponse::Error { error }) => {
                    return Err(BackendError::Other(error));
                }
                Ok(_) => {}
                Err(e) => log::warn!("unparseable guest response: {e}: {}", buf.trim()),
            }
        }
        Ok(exit_code)
    }

    async fn remove_image(&self, reference: &str) -> Result<(), BackendError> {
        let cmd = GuestCommand::ImageRm {
            reference: reference.to_string(),
        };
        let stdout = self.roundtrip(&cmd).await?;
        let _ = stdout;
        Ok(())
    }

    async fn kubernetes_status(&self) -> Result<bool, BackendError> {
        let stream = self.connect().await?;
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let mut line = serde_json::to_string(&GuestCommand::KubernetesStatus).map_err(BackendError::ParseError)?;
        line.push('\n');
        writer.write_all(line.as_bytes()).await.map_err(BackendError::Io)?;

        let mut buf = String::new();
        loop {
            buf.clear();
            if reader.read_line(&mut buf).await.map_err(BackendError::Io)? == 0 { break; }
            match serde_json::from_str::<GuestResponse>(buf.trim()) {
                Ok(GuestResponse::KubernetesStatus { running }) => return Ok(running),
                Ok(GuestResponse::Error { error }) => return Err(BackendError::Other(error)),
                Ok(GuestResponse::Exit { .. }) => break,
                Ok(_) => {}
                Err(e) => log::warn!("unparseable guest response: {e}: {}", buf.trim()),
            }
        }
        Ok(false)
    }

    async fn start_kubernetes(&self, tx: UnboundedSender<String>) -> Result<(), BackendError> {
        let cmd = GuestCommand::KubernetesStart;
        let stream = self.connect().await?;
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let mut line = serde_json::to_string(&cmd).map_err(BackendError::ParseError)?;
        line.push('\n');
        writer.write_all(line.as_bytes()).await.map_err(BackendError::Io)?;

        let mut buf = String::new();
        loop {
            buf.clear();
            if reader.read_line(&mut buf).await.map_err(BackendError::Io)? == 0 { break; }
            match serde_json::from_str::<GuestResponse>(buf.trim()) {
                Ok(GuestResponse::Stream { data, .. }) => {
                    for l in data.lines() { let _ = tx.send(l.to_string()); }
                }
                Ok(GuestResponse::Exit { .. }) => break,
                Ok(GuestResponse::Error { error }) => return Err(BackendError::Other(error)),
                Ok(_) => {}
                Err(e) => log::warn!("unparseable guest response: {e}: {}", buf.trim()),
            }
        }
        Ok(())
    }

    async fn stop_kubernetes(&self) -> Result<(), BackendError> {
        self.roundtrip(&GuestCommand::KubernetesStop).await?;
        Ok(())
    }

    async fn stream_logs(
        &self,
        name: &str,
        follow: bool,
        tx: UnboundedSender<String>,
    ) -> Result<(), BackendError> {
        let cmd = GuestCommand::Logs {
            name: name.to_string(),
            follow,
        };

        let stream = self.connect().await?;
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let mut line = serde_json::to_string(&cmd).map_err(BackendError::ParseError)?;
        line.push('\n');
        writer
            .write_all(line.as_bytes())
            .await
            .map_err(BackendError::Io)?;

        let mut buf = String::new();
        loop {
            buf.clear();
            let n = reader.read_line(&mut buf).await.map_err(BackendError::Io)?;
            if n == 0 {
                break;
            }
            match serde_json::from_str::<GuestResponse>(buf.trim()) {
                Ok(GuestResponse::Stream { data, .. }) => {
                    for l in data.lines() {
                        if tx.send(l.to_string()).is_err() {
                            return Ok(());
                        }
                    }
                }
                Ok(GuestResponse::Exit { .. }) => break,
                Ok(GuestResponse::Error { error }) => {
                    return Err(BackendError::Other(error));
                }
                Ok(_) => {}
                Err(e) => log::warn!("unparseable guest response: {e}: {}", buf.trim()),
            }
        }
        Ok(())
    }
}
