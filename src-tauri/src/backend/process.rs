//! Linux backend: communicates with pelagos by spawning CLI subprocesses.
//!
//! Requires `pelagos` to be in PATH.  Rootless operations (ps, stop, rm)
//! work without sudo.

use super::{BackendError, RuntimeBackend};
use pelagos_protocol::{ContainerInfo, GuestMount, ImageInfo};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

// Stable flag for JSON output.  Update to "--json" once pelagos#108 lands.
const PS_JSON_ARGS: &[&str] = &["ps", "--all", "--format", "json"];

pub struct ProcessBackend {
    /// Resolved path to the pelagos binary, or None if unavailable.
    bin: Option<std::path::PathBuf>,
}

impl ProcessBackend {
    /// Locate `pelagos` in PATH and return a backend instance.
    pub fn new() -> Result<Self, BackendError> {
        let bin = which::which("pelagos").map_err(|_| BackendError::BinaryNotFound)?;
        log::info!("pelagos backend: using binary at {}", bin.display());
        Ok(Self { bin: Some(bin) })
    }

    /// Return a backend that reports BinaryNotFound for every operation.
    /// Used when pelagos is not installed so the UI can show an error state
    /// rather than panic on startup.
    pub fn unavailable() -> Self {
        Self { bin: None }
    }

    async fn run(&self, args: &[&str]) -> Result<(String, String, i32), BackendError> {
        let bin = self.bin.as_ref().ok_or(BackendError::BinaryNotFound)?;
        let out = Command::new(bin)
            .args(args)
            .output()
            .await
            .map_err(BackendError::Io)?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let code = out.status.code().unwrap_or(-1);
        Ok((stdout, stderr, code))
    }
}

#[async_trait::async_trait]
impl RuntimeBackend for ProcessBackend {
    async fn list_containers(&self) -> Result<Vec<ContainerInfo>, BackendError> {
        let (stdout, stderr, code) = self.run(PS_JSON_ARGS).await?;
        if code != 0 {
            return Err(BackendError::CommandFailed { code, stderr });
        }
        let containers: Vec<ContainerInfo> = serde_json::from_str(&stdout)?;
        Ok(containers)
    }

    async fn stop_container(&self, name: &str) -> Result<(), BackendError> {
        let (_, stderr, code) = self.run(&["stop", name]).await?;
        if code != 0 {
            return Err(BackendError::CommandFailed { code, stderr });
        }
        Ok(())
    }

    async fn remove_container(&self, name: &str, force: bool) -> Result<(), BackendError> {
        let mut args = vec!["rm"];
        if force {
            args.push("--force");
        }
        args.push(name);
        let (_, stderr, code) = self.run(&args).await?;
        if code != 0 {
            return Err(BackendError::CommandFailed { code, stderr });
        }
        Ok(())
    }

    async fn ping(&self) -> bool {
        self.run(&["--version"])
            .await
            .map(|(_, _, c)| c == 0)
            .unwrap_or(false)
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
        let bin = self.bin.as_ref().ok_or(BackendError::BinaryNotFound)?;
        let mut cmd = Command::new(bin);
        cmd.arg("run");
        if let Some(n) = name {
            cmd.arg("--name").arg(n);
        }
        if detach {
            cmd.arg("--detach");
        }
        if !ports.is_empty() {
            for p in &ports {
                cmd.arg("--publish").arg(p);
            }
            cmd.arg("--network").arg("pasta");
        }
        for m in &mounts {
            let src = if m.subpath.is_empty() {
                format!("/mnt/{}", m.tag)
            } else {
                format!("/mnt/{}/{}", m.tag, m.subpath)
            };
            let spec = if m.read_only {
                format!("{}:{}:ro", src, m.container_path)
            } else {
                format!("{}:{}", src, m.container_path)
            };
            cmd.arg("--mount").arg(spec);
        }
        cmd.arg(image);
        for a in &args {
            cmd.arg(a);
        }
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(BackendError::Io)?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let tx2 = tx.clone();
        let h1 = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(line);
            }
        });
        let h2 = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx2.send(line);
            }
        });

        let _ = tokio::join!(h1, h2);
        let status = child.wait().await.map_err(BackendError::Io)?;
        Ok(status.code().unwrap_or(-1))
    }

    async fn list_images(&self) -> Result<Vec<ImageInfo>, BackendError> {
        let (stdout, stderr, code) = self.run(&["image", "ls", "--json"]).await?;
        if code != 0 {
            return Err(BackendError::CommandFailed { code, stderr });
        }
        Ok(serde_json::from_str(&stdout)?)
    }

    async fn pull_image(
        &self,
        reference: &str,
        tx: UnboundedSender<String>,
    ) -> Result<i32, BackendError> {
        let bin = self.bin.as_ref().ok_or(BackendError::BinaryNotFound)?;
        let mut cmd = Command::new(bin);
        cmd.args(["image", "pull", reference])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(BackendError::Io)?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let tx2 = tx.clone();
        let h1 = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx.send(line);
            }
        });
        let h2 = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx2.send(line);
            }
        });

        let _ = tokio::join!(h1, h2);
        let status = child.wait().await.map_err(BackendError::Io)?;
        Ok(status.code().unwrap_or(-1))
    }

    async fn remove_image(&self, reference: &str) -> Result<(), BackendError> {
        let (_, stderr, code) = self.run(&["image", "rm", reference]).await?;
        if code != 0 {
            return Err(BackendError::CommandFailed { code, stderr });
        }
        Ok(())
    }

    async fn stream_logs(
        &self,
        name: &str,
        follow: bool,
        tx: UnboundedSender<String>,
    ) -> Result<(), BackendError> {
        let bin = self.bin.as_ref().ok_or(BackendError::BinaryNotFound)?;
        let mut cmd = Command::new(bin);
        cmd.arg("logs");
        if follow {
            cmd.arg("--follow");
        }
        cmd.arg(name);
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(BackendError::Io)?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let tx2 = tx.clone();
        let h1 = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let h2 = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx2.send(line).is_err() {
                    break;
                }
            }
        });

        let _ = tokio::join!(h1, h2);
        child.wait().await.map_err(BackendError::Io)?;
        Ok(())
    }

    async fn kubernetes_status(&self) -> Result<bool, BackendError> {
        let api = std::process::Command::new("pgrep")
            .args(["-x", "api-server"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let kubelet = std::process::Command::new("pgrep")
            .args(["-x", "kubelet"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        Ok(api && kubelet)
    }

    async fn start_kubernetes(&self, tx: UnboundedSender<String>) -> Result<(), BackendError> {
        let dockerd = find_bin("pelagos-dockerd", &["/usr/local/bin", "/mnt/Projects/pelagos/target/debug"]);
        let api_bin = find_bin("api-server", &["/usr/local/bin", "/mnt/Projects/rusternetes/target/debug"]);
        let kubelet_bin = find_bin("kubelet", &["/usr/local/bin", "/mnt/Projects/rusternetes/target/debug"]);
        let data_dir = "/var/lib/rusternetes/cluster.db";
        std::fs::create_dir_all("/var/lib/rusternetes").map_err(BackendError::Io)?;

        // pelagos-dockerd
        if !is_running("pelagos-dockerd") {
            let pelagos = self.bin.as_ref().map(|p| p.as_os_str().to_string_lossy().into_owned()).unwrap_or_else(|| "pelagos".to_string());
            std::process::Command::new(&dockerd)
                .arg("--pelagos-bin").arg(&pelagos)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(BackendError::Io)?;
            let _ = tx.send("started pelagos-dockerd".into());
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // api-server
        if !is_running("api-server") {
            std::process::Command::new(&api_bin)
                .args(["--storage-backend", "sqlite", "--data-dir", data_dir,
                       "--skip-auth", "--tls", "--tls-self-signed",
                       "--tls-san", "localhost,127.0.0.1"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(BackendError::Io)?;
            let _ = tx.send("started api-server, waiting for ready...".into());
            wait_for_port(6443, 30).await;
            let _ = tx.send("api-server ready".into());
        }

        // kubelet
        if !is_running("kubelet") {
            std::process::Command::new(&kubelet_bin)
                .args(["--node-name", "pelagos-node",
                       "--storage-backend", "sqlite",
                       "--data-dir", data_dir,
                       "--network", "bridge"])
                .env("DOCKER_HOST", "unix:///var/run/pelagos-dockerd.sock")
                .env("RUST_MIN_STACK", "8388608")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(BackendError::Io)?;
            let _ = tx.send("started kubelet".into());
        }

        Ok(())
    }

    async fn stop_kubernetes(&self) -> Result<(), BackendError> {
        for name in &["kubelet", "api-server", "pelagos-dockerd"] {
            let _ = std::process::Command::new("pkill").args(["-x", name]).status();
        }
        Ok(())
    }
}

fn is_running(name: &str) -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", name])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn find_bin(name: &str, dirs: &[&str]) -> String {
    for dir in dirs {
        let p = format!("{}/{}", dir, name);
        if std::path::Path::new(&p).exists() {
            return p;
        }
    }
    name.to_string()
}

async fn wait_for_port(port: u16, timeout_secs: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    while tokio::time::Instant::now() < deadline {
        if tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_backend_has_no_bin() {
        let b = ProcessBackend::unavailable();
        assert!(b.bin.is_none());
    }

    #[tokio::test]
    async fn unavailable_backend_returns_error() {
        let b = ProcessBackend::unavailable();
        assert!(matches!(
            b.list_containers().await,
            Err(BackendError::BinaryNotFound)
        ));
    }
}
