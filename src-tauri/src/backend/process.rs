//! Linux backend: communicates with pelagos by spawning CLI subprocesses.
//!
//! Requires `pelagos` to be in PATH.  Rootless operations (ps, stop, rm)
//! work without sudo.

use super::{BackendError, RuntimeBackend};
use pelagos_protocol::{ContainerInfo, GuestMount, ImageInfo};
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
