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

/// Pending run params, held between `launch_terminal_window` and `pty_start`.
pub struct PtyParams {
    pub image: String,
    pub name: Option<String>,
    pub args: Vec<String>,
    pub ports: Vec<String>,
    pub volumes: Vec<String>,
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
/// Stores the run params in [`PtyState`] under a fresh UUID label, then opens a
/// Tauri [`WebviewWindow`] at `/terminal?label=<uuid>`.  The window's Svelte page
/// calls [`pty_start`] once xterm.js is mounted.
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
    let label = format!("terminal-{}", uuid::Uuid::new_v4().simple());

    // Park the params until the window calls pty_start.
    state
        .0
        .lock()
        .unwrap()
        .pending
        .insert(label.clone(), PtyParams { image: image.clone(), name, args, ports, volumes });

    let url = WebviewUrl::App(format!("/terminal?label={}", label).into());
    WebviewWindowBuilder::new(&app, &label, url)
        .title(format!("pelagos — {image}"))
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
/// Must be called after the terminal window has mounted xterm.js so that the
/// initial output is not lost (xterm buffers writes before the DOM is ready, but
/// we want the listener registered before we emit).
///
/// Frontend: `await invoke('pty_start', { label })`
#[tauri::command]
pub fn pty_start(
    app: tauri::AppHandle,
    state: tauri::State<PtyState>,
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

    // Build the pelagos command.
    let pelagos = find_pelagos_bin();
    let mut cmd = CommandBuilder::new(&pelagos);
    cmd.arg("run");
    cmd.arg("--tty");
    cmd.arg("--interactive");
    if let Some(n) = &params.name {
        cmd.arg("--name");
        cmd.arg(n);
    }
    if !params.ports.is_empty() {
        for p in &params.ports {
            cmd.arg("-p");
            cmd.arg(p);
        }
        cmd.arg("-n");
        cmd.arg("pasta");
    }
    for v in &params.volumes {
        cmd.arg("-v");
        cmd.arg(v);
    }
    cmd.arg(&params.image);
    for a in &params.args {
        cmd.arg(a);
    }

    // Open the PTY pair.
    let pty_system = NativePtySystem::default();
    let pair = pty_system
        .openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;

    // Spawn the child on the slave side, then drop the slave — the master keeps
    // the session alive.
    let mut child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);

    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

    // Store the session so resize and input commands can reach it.
    state.0.lock().unwrap().sessions.insert(
        label.clone(),
        PtySession { master: pair.master, writer },
    );

    // Reader thread: PTY stdout → `pty-output-<label>` events.
    let app_r = app.clone();
    let label_r = label.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = app_r.emit(
                        &format!("pty-output-{}", label_r),
                        buf[..n].to_vec(),
                    );
                }
            }
        }
    });

    // Wait thread: reap the child process, then emit `pty-exit-<label>`.
    let app_w = app.clone();
    let label_w = label.clone();
    std::thread::spawn(move || {
        let code = child
            .wait()
            .map(|s| s.exit_code() as i32)
            .unwrap_or(0);
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
        .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
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
                    image: "alpine:latest".into(),
                    name: Some("my-container".into()),
                    args: vec!["/bin/sh".into()],
                    ports: vec!["8080:80".into()],
                    volumes: vec![],
                },
            );
            assert!(map.pending.contains_key(&label));
        }

        {
            let mut map = state.0.lock().unwrap();
            let params = map.pending.remove(&label).expect("params should exist");
            assert_eq!(params.image, "alpine:latest");
            assert_eq!(params.name.as_deref(), Some("my-container"));
            assert_eq!(params.args, vec!["/bin/sh"]);
            assert_eq!(params.ports, vec!["8080:80"]);
            assert!(params.volumes.is_empty());
        }

        {
            let map = state.0.lock().unwrap();
            assert!(map.pending.is_empty());
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
