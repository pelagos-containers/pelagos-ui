//! Launch interactive container sessions in a new terminal window.
//!
//! macOS default: write a `.terminal` plist and `open` it with Terminal.app.
//! `.terminal` files are opened with `RunCommandAsShell false`, which bypasses
//! login-shell initialisation (oh-my-zsh, p10k, etc.) entirely.
//! This avoids the bug where an interactive startup prompt (e.g. oh-my-zsh
//! auto-update: "Would you like to update? [Y/n]") reads the leading `/` of
//! the command path from the PTY, leaving a broken relative path to execute.
//!
//! `.command` files were the previous approach; they require Terminal to send
//! the path as keyboard input to the login shell, which is intercepted by any
//! shell plugin that reads from the TTY during startup.
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
            // Apple Terminal, Warp, unknown: use osascript do script.
            // The app is not sandboxed so macOS will prompt once for
            // Automation permission; after that it works without any
            // login-shell initialisation (no oh-my-zsh interference).
            _ => osascript_terminal(&cmd),
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

/// Open a new Terminal.app window and run cmd via osascript `do script`.
///
/// oh-my-zsh (and similar plugins) intercept the first keystroke from the
/// PTY during shell startup with `read -r -k 1` for their auto-update prompt.
/// We prepend `:\n` to the command: `:` is consumed by that read (it is not
/// `Y`, so the update is skipped), and `:` is also a valid POSIX no-op so if
/// no prompt is active it executes harmlessly.  The real command follows on
/// the next line.
///
/// The app is not sandboxed, so macOS shows a one-time Automation permission
/// prompt the first time; subsequent calls are silent.
#[cfg(target_os = "macos")]
fn osascript_terminal(cmd: &str) -> Result<(), String> {
    // ":" & linefeed answers any single-char TTY prompt (oh-my-zsh update,
    // p10k instant-prompt, etc.) then the real command runs on the next line.
    let script = format!(
        "tell application \"Terminal\"\n\
         \tdo script (\":\" & linefeed & \"{}\")\n\
         \tactivate\n\
         end tell",
        escape_applescript(cmd)
    );
    std::process::Command::new("osascript")
        .args(["-e", &script])
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
