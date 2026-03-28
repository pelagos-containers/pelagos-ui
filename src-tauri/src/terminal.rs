//! Launch interactive container sessions in a new terminal window.
//!
//! macOS default: write a self-deleting `.command` file and `open` it.
//! `.command` files are opened by Terminal.app without requiring the
//! Automation permission that `osascript "do script"` demands.
//!
//! iTerm / ghostty / kitty / alacritty: detected via $TERM_PROGRAM or
//! $PELAGOS_TERMINAL override; launched via their own CLI flags.

/// Build and launch `pelagos run --tty --interactive [--name N] image [args]`
/// in a new terminal window.
pub fn open_in_terminal(
    image: &str,
    name: Option<&str>,
    args: &[String],
    ports: &[String],
    volumes: &[String],
) -> Result<(), String> {
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
    if !ports.is_empty() {
        for p in ports {
            parts.push("-p".into());
            parts.push(shell_quote(p));
        }
        parts.push("-n".into());
        parts.push("pasta".into());
    }
    for v in volumes {
        parts.push("-v".into());
        parts.push(shell_quote(v));
    }
    parts.push(shell_quote(image));
    for a in args {
        parts.push(shell_quote(a));
    }
    let cmd = parts.join(" ");

    // $PELAGOS_TERMINAL overrides everything.
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
            // Apple Terminal, Warp, unknown: use .command file — no
            // Automation permission needed, works in any sandbox level.
            _ => open_command_file(&cmd),
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
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

/// Write a self-deleting `.command` file to /tmp and open it.
///
/// macOS associates `.command` files with Terminal.app; `open` launches them
/// without requiring the Automation entitlement that `osascript` needs.
/// The script removes itself on startup so /tmp stays tidy.
#[cfg(target_os = "macos")]
fn open_command_file(cmd: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let path = std::env::temp_dir().join(format!("pelagos-run-{}.command", std::process::id()));
    let mut f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    // Self-delete then exec the pelagos command so the terminal inherits it cleanly.
    writeln!(f, "#!/bin/bash").map_err(|e| e.to_string())?;
    writeln!(f, "rm -- \"$0\"").map_err(|e| e.to_string())?;
    writeln!(f, "exec {}", cmd).map_err(|e| e.to_string())?;
    drop(f);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;

    std::process::Command::new("open")
        .arg(&path)
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

/// Locate the `pelagos` host binary.
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
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Single-quote a shell argument, escaping any embedded single quotes.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
