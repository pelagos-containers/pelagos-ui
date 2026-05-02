//! Embedded PTY for interactive container sessions.
//!
//! Each "Run Interactive" click opens a new Tauri window loading the `/terminal`
//! route.  The window calls [`pty_start`] which spawns `pelagos run --tty
//! --interactive` as a direct child of a PTY allocated with `portable-pty`.
//! Output flows from PTY → Tauri event → xterm.js; input flows in reverse.
//! No login shell is involved, so oh-my-zsh and similar plugins cannot interfere.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use tauri::{Emitter, WebviewUrl, WebviewWindowBuilder};

// ── State ─────────────────────────────────────────────────────────────────────

/// What kind of pelagos session to spawn in a terminal window.
pub enum PtyMode {
    /// `pelagos run --tty --interactive [opts] <image> [args]`
    Run {
        image: String,
        name: Option<String>,
        args: Vec<String>,
        ports: Vec<String>,
        volumes: Vec<String>,
    },
    /// `pelagos exec -i <container> [cmd]`
    Exec {
        container: String,
        cmd: Vec<String>,
    },
}

/// Pending session params, held between `launch_*_window` and `pty_start`.
pub struct PtyParams {
    pub mode: PtyMode,
}

/// Live PTY session — kept alive for resize and input.
pub struct PtySession {
    /// Master side of the PTY — used for resize.
    pub master: Box<dyn portable_pty::MasterPty + Send>,
    /// Write half of the master — used for keyboard input.
    pub writer: Box<dyn Write + Send>,
}

pub struct PtyMap {
    pub pending: HashMap<String, PtyParams>,
    pub sessions: HashMap<String, PtySession>,
}

/// Managed Tauri state — one instance for the whole app lifetime.
pub struct PtyState(pub Mutex<PtyMap>);

impl PtyState {
    pub fn new() -> Self {
        PtyState(Mutex::new(PtyMap {
            pending: HashMap::new(),
            sessions: HashMap::new(),
        }))
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Open a new terminal window for `pelagos run --tty --interactive image [args]`.
///
/// Frontend: `await invoke('launch_terminal_window', { image, name, args, ports, volumes })`
#[tauri::command]
pub fn launch_terminal_window(
    app: tauri::AppHandle,
    state: tauri::State<PtyState>,
    image: String,
    name: Option<String>,
    args: Vec<String>,
    ports: Vec<String>,
    volumes: Vec<String>,
) -> Result<String, String> {
    let title = format!("pelagos — {image}");
    open_terminal_window(&app, &state, title, PtyMode::Run { image, name, args, ports, volumes })
}

/// Open a new terminal window for `pelagos exec -i <container> [cmd]`.
///
/// `cmd` defaults to `["sh"]` when empty.
///
/// Frontend: `await invoke('launch_exec_window', { container, cmd })`
#[tauri::command]
pub fn launch_exec_window(
    app: tauri::AppHandle,
    state: tauri::State<PtyState>,
    container: String,
    cmd: Vec<String>,
) -> Result<String, String> {
    let title = format!("pelagos — exec {container}");
    open_terminal_window(&app, &state, title, PtyMode::Exec { container, cmd })
}

/// Shared implementation: park params and open a terminal WebviewWindow.
fn open_terminal_window(
    app: &tauri::AppHandle,
    state: &tauri::State<PtyState>,
    title: String,
    mode: PtyMode,
) -> Result<String, String> {
    let label = format!("terminal-{}", uuid::Uuid::new_v4().simple());
    state.0.lock().unwrap().pending.insert(label.clone(), PtyParams { mode });
    let url = WebviewUrl::App(format!("/terminal?label={}", label).into());
    WebviewWindowBuilder::new(app, &label, url)
        .title(title)
        .inner_size(900.0, 600.0)
        .min_inner_size(400.0, 300.0)
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(label)
}

/// Allocate a PTY, spawn `pelagos run --tty --interactive`, and start bridging
/// PTY output to `pty-output-<label>` Tauri events.
///
/// Async so that the Tauri IPC thread (which also delivers events to WKWebView)
/// is not blocked during PTY allocation and process spawn.  The blocking work
/// runs in `tokio::task::spawn_blocking`.
///
/// Must be called after the terminal window has mounted xterm.js so that the
/// initial output is not lost (xterm buffers writes before the DOM is ready, but
/// we want the listener registered before we emit).
///
/// Frontend: `await invoke('pty_start', { label })`
#[tauri::command]
pub async fn pty_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, PtyState>,
    label: String,
) -> Result<(), String> {
    // Take the pending params — error if the label is unknown or already started.
    let params = state
        .0
        .lock()
        .unwrap()
        .pending
        .remove(&label)
        .ok_or_else(|| format!("no pending PTY for label {label}"))?;

    // Build the command args before entering spawn_blocking.
    let pelagos = find_pelagos_bin();
    let mut cmd = CommandBuilder::new(&pelagos);
    match &params.mode {
        PtyMode::Run { image, name, args, ports, volumes } => {
            cmd.arg("run");
            cmd.arg("--tty");
            cmd.arg("--interactive");
            if let Some(n) = name {
                cmd.arg("--name");
                cmd.arg(n);
            }
            if !ports.is_empty() {
                for p in ports {
                    cmd.arg("-p");
                    cmd.arg(p);
                }
                cmd.arg("-n");
                cmd.arg("pasta");
            }
            for v in volumes {
                cmd.arg("-v");
                cmd.arg(v);
            }
            cmd.arg(image);
            for a in args {
                cmd.arg(a);
            }
        }
        PtyMode::Exec { container, cmd: exec_cmd } => {
            cmd.arg("exec");
            cmd.arg("-i");
            cmd.arg(container);
            if exec_cmd.is_empty() {
                cmd.arg("sh");
            } else {
                for a in exec_cmd {
                    cmd.arg(a);
                }
            }
        }
    }

    // PTY allocation and process spawn are blocking syscalls (openpty, fork,
    // exec).  Run them on a dedicated blocking thread so the tokio runtime and
    // Tauri's IPC/event delivery remain responsive.
    type PtyHandles = (
        Box<dyn portable_pty::MasterPty + Send>,
        Box<dyn Write + Send>,
        Box<dyn Read + Send>,
        Box<dyn portable_pty::Child + Send + Sync>,
    );

    let (master, writer, reader, child): PtyHandles =
        tokio::task::spawn_blocking(move || -> Result<PtyHandles, String> {
            let pty_system = NativePtySystem::default();
            let pair = pty_system
                .openpty(PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| e.to_string())?;

            let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
            drop(pair.slave);

            let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
            let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
            Ok((pair.master, writer, reader, child))
        })
        .await
        .map_err(|e| e.to_string())??;

    // Store the session so resize and input commands can reach it.
    state
        .0
        .lock()
        .unwrap()
        .sessions
        .insert(label.clone(), PtySession { master, writer });

    // Reader thread: PTY stdout → `pty-output-<label>` events.
    let app_r = app.clone();
    let label_r = label.clone();
    let mut reader = reader;
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = app_r.emit(&format!("pty-output-{}", label_r), buf[..n].to_vec());
                }
            }
        }
    });

    // Wait thread: reap the child process, then emit `pty-exit-<label>`.
    let app_w = app.clone();
    let label_w = label.clone();
    let mut child = child;
    std::thread::spawn(move || {
        let code = child.wait().map(|s| s.exit_code() as i32).unwrap_or(0);
        let _ = app_w.emit(&format!("pty-exit-{}", label_w), code);
    });

    Ok(())
}

/// Write raw bytes (keyboard input) to the PTY master.
///
/// Frontend: `terminal.onData(s => invoke('pty_input', { label, data: [...new TextEncoder().encode(s)] }))`
#[tauri::command]
pub fn pty_input(
    state: tauri::State<PtyState>,
    label: String,
    data: Vec<u8>,
) -> Result<(), String> {
    let mut map = state.0.lock().unwrap();
    let session = map
        .sessions
        .get_mut(&label)
        .ok_or_else(|| format!("no PTY session for label {label}"))?;
    session.writer.write_all(&data).map_err(|e| e.to_string())
}

/// Resize the PTY after a window resize or xterm fit.
///
/// Frontend: `invoke('pty_resize', { label, cols: terminal.cols, rows: terminal.rows })`
#[tauri::command]
pub fn pty_resize(
    state: tauri::State<PtyState>,
    label: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let map = state.0.lock().unwrap();
    let session = map
        .sessions
        .get(&label)
        .ok_or_else(|| format!("no PTY session for label {label}"))?;
    session
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())
}

/// Clean up a PTY session.  Dropping `PtySession` closes the master FD, which
/// sends SIGHUP to the child.  The wait thread will then reap the child.
///
/// Called by the terminal window's `onDestroy` and by the `on_window_event`
/// handler on `CloseRequested`.
///
/// Frontend: `invoke('pty_close', { label })`
#[tauri::command]
pub fn pty_close(state: tauri::State<PtyState>, label: String) -> Result<(), String> {
    let mut map = state.0.lock().unwrap();
    map.sessions.remove(&label);
    map.pending.remove(&label);
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Locate the `pelagos` host binary.
///
/// macOS GUI apps launch with a stripped PATH that excludes Homebrew, so we
/// check the two canonical Homebrew prefix locations before falling back to
/// a PATH search.
pub fn find_pelagos_bin() -> String {
    for candidate in &["/opt/homebrew/bin/pelagos", "/usr/local/bin/pelagos"] {
        if std::path::Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }
    which::which("pelagos")
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "pelagos".into())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pty_state_starts_empty() {
        let state = PtyState::new();
        let map = state.0.lock().unwrap();
        assert!(map.pending.is_empty());
        assert!(map.sessions.is_empty());
    }

    #[test]
    fn find_pelagos_bin_returns_nonempty_string() {
        let bin = find_pelagos_bin();
        assert!(!bin.is_empty());
    }

    #[test]
    fn pending_params_insert_and_remove() {
        let state = PtyState::new();
        let label = "terminal-test-abc".to_string();

        {
            let mut map = state.0.lock().unwrap();
            map.pending.insert(
                label.clone(),
                PtyParams {
                    mode: PtyMode::Run {
                        image: "alpine:latest".into(),
                        name: Some("my-container".into()),
                        args: vec!["/bin/sh".into()],
                        ports: vec!["8080:80".into()],
                        volumes: vec![],
                    },
                },
            );
            assert!(map.pending.contains_key(&label));
        }

        {
            let mut map = state.0.lock().unwrap();
            let params = map.pending.remove(&label).expect("params should exist");
            match params.mode {
                PtyMode::Run { image, name, args, ports, volumes } => {
                    assert_eq!(image, "alpine:latest");
                    assert_eq!(name.as_deref(), Some("my-container"));
                    assert_eq!(args, vec!["/bin/sh"]);
                    assert_eq!(ports, vec!["8080:80"]);
                    assert!(volumes.is_empty());
                }
                _ => panic!("expected Run mode"),
            }
        }

        {
            let map = state.0.lock().unwrap();
            assert!(map.pending.is_empty());
        }
    }

    #[test]
    fn pending_params_exec_mode() {
        let state = PtyState::new();
        let label = "terminal-exec-test".to_string();

        {
            let mut map = state.0.lock().unwrap();
            map.pending.insert(
                label.clone(),
                PtyParams {
                    mode: PtyMode::Exec {
                        container: "my-container".into(),
                        cmd: vec!["bash".into()],
                    },
                },
            );
        }

        {
            let mut map = state.0.lock().unwrap();
            let params = map.pending.remove(&label).expect("params should exist");
            match params.mode {
                PtyMode::Exec { container, cmd } => {
                    assert_eq!(container, "my-container");
                    assert_eq!(cmd, vec!["bash"]);
                }
                _ => panic!("expected Exec mode"),
            }
        }
    }

    #[test]
    fn pty_close_unknown_label_is_noop() {
        // pty_close on an unknown label must not panic — it silently removes
        // nothing.
        let state = PtyState::new();
        {
            let mut map = state.0.lock().unwrap();
            map.sessions.remove("terminal-nonexistent");
            map.pending.remove("terminal-nonexistent");
        }
        // No panic → pass.
    }

    #[test]
    fn label_format_is_prefixed() {
        // Verify that labels we generate start with "terminal-" (important for
        // the on_window_event handler that uses starts_with to distinguish them
        // from the "main" window).
        let label = format!("terminal-{}", uuid::Uuid::new_v4().simple());
        assert!(label.starts_with("terminal-"));
        assert!(label.len() > "terminal-".len());
    }
}
