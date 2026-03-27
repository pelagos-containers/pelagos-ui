//! Launch interactive container sessions in a new terminal window.
//!
//! Ported from pelagos-tui's open_in_terminal logic.
//! Terminal is detected via $PELAGOS_TERMINAL override or $TERM_PROGRAM,
//! falling back to Apple Terminal on macOS / xterm on Linux.

/// Build and launch `pelagos run --tty --interactive [--name N] image [args]`
/// in a new terminal window.
pub fn open_in_terminal(image: &str, name: Option<&str>, args: &[String]) -> Result<(), String> {
    let pelagos = find_pelagos_bin();
    let mut parts: Vec<String> = vec![
        shell_quote(&pelagos),
        "run".into(),
        "--tty".into(),
        "--interactive".into(),
    ];
    if let Some(n) = name {
        parts.push("--name".into());
        parts.push(shell_quote(n));
    }
    parts.push(shell_quote(image));
    for a in args {
        parts.push(shell_quote(a));
    }
    let cmd = parts.join(" ");

    if let Ok(term_bin) = std::env::var("PELAGOS_TERMINAL") {
        return spawn_generic(&term_bin, &cmd);
    }

    #[cfg(target_os = "macos")]
    {
        let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();
        match term_program.as_str() {
            "iTerm.app" => osascript_iterm(&cmd),
            "ghostty" => spawn_generic("ghostty", &cmd),
            "kitty" => spawn_generic("kitty", &cmd),
            "alacritty" => spawn_generic("alacritty", &cmd),
            // Apple_Terminal, WarpTerminal, unknown → AppleScript Terminal
            _ => osascript_apple_terminal(&cmd),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Linux: try common terminals in order.
        for term in &[
            "x-terminal-emulator",
            "gnome-terminal",
            "xfce4-terminal",
            "xterm",
        ] {
            if which::which(term).is_ok() {
                return spawn_generic(term, &cmd);
            }
        }
        Err("no terminal emulator found — set $PELAGOS_TERMINAL".into())
    }
}

/// Locate the `pelagos` host binary (same logic as lib.rs find_pelagos_bin).
fn find_pelagos_bin() -> String {
    for candidate in &["/opt/homebrew/bin/pelagos", "/usr/local/bin/pelagos"] {
        if std::path::Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }
    which::which("pelagos")
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "pelagos".into())
}

#[cfg(target_os = "macos")]
fn osascript_apple_terminal(cmd: &str) -> Result<(), String> {
    let script = format!(
        "tell application \"Terminal\" to do script \"{}\"",
        escape_applescript(cmd)
    );
    std::process::Command::new("osascript")
        .args(["-e", &script])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn osascript_iterm(cmd: &str) -> Result<(), String> {
    let script = format!(
        "tell application \"iTerm\" to create window with default profile command \"{}\"",
        escape_applescript(cmd)
    );
    std::process::Command::new("osascript")
        .args(["-e", &script])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn spawn_generic(term_bin: &str, cmd: &str) -> Result<(), String> {
    std::process::Command::new(term_bin)
        .args(["-e", "sh", "-c", cmd])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Single-quote a shell argument, escaping any embedded single quotes.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
