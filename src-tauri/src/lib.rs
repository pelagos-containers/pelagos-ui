mod backend;
mod commands;
mod terminal;

use std::sync::Arc;
use std::time::Duration;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

const ICON_RUNNING: &[u8] = include_bytes!("../icons/tray-running.png");
const ICON_STOPPED: &[u8] = include_bytes!("../icons/tray-stopped.png");

pub fn run() {
    env_logger::init();

    let backend: Arc<dyn backend::RuntimeBackend> = make_backend();

    tauri::Builder::default()
        .manage(backend)
        .invoke_handler(tauri::generate_handler![
            commands::list_containers,
            commands::stop_container,
            commands::remove_container,
            commands::ping,
            commands::vm_status,
            commands::run_container,
            commands::launch_interactive,
        ])
        .setup(|app| {
            // Inject GTK CSS on Linux so the tray context menu has an opaque
            // background regardless of the compositor's transparency settings.
            // Uses catppuccin-mocha colours to match Omarchy/Waybar.
            #[cfg(target_os = "linux")]
            inject_menu_css();

            let open = MenuItem::with_id(app, "open", "Open Dashboard", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

            // On macOS, add VM start/stop items.  Initial state: Start enabled, Stop
            // disabled (the polling loop corrects this on its first tick).
            #[cfg(target_os = "macos")]
            let vm_start = MenuItem::with_id(app, "start-vm", "Start VM", true, None::<&str>)?;
            #[cfg(target_os = "macos")]
            let vm_stop = MenuItem::with_id(app, "stop-vm", "Stop VM", false, None::<&str>)?;

            #[cfg(target_os = "macos")]
            let menu = {
                let sep2 = PredefinedMenuItem::separator(app)?;
                Menu::with_items(app, &[&open, &sep, &vm_start, &vm_stop, &sep2, &quit])?
            };

            #[cfg(not(target_os = "macos"))]
            let menu = Menu::with_items(app, &[&open, &sep, &quit])?;

            TrayIconBuilder::with_id("main-tray")
                // Template image: macOS colours the icon for light/dark mode automatically.
                .icon_as_template(true)
                .icon(tauri::image::Image::from_bytes(ICON_RUNNING)?)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main_window(app),
                    "quit" => app.exit(0),
                    #[cfg(target_os = "macos")]
                    "start-vm" => spawn_vm_cmd("start"),
                    #[cfg(target_os = "macos")]
                    "stop-vm" => spawn_vm_cmd("stop"),
                    _ => {}
                })
                .on_tray_icon_event(|tray: &tauri::tray::TrayIcon<_>, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // Clone item handles for the background polling task (macOS only).
            #[cfg(target_os = "macos")]
            let (vm_start_task, vm_stop_task) = (vm_start.clone(), vm_stop.clone());

            let backend = app
                .state::<Arc<dyn backend::RuntimeBackend>>()
                .inner()
                .clone();
            let app_handle = app.handle().clone();

            tauri::async_runtime::spawn(async move {
                loop {
                    let running = backend.ping().await;

                    if let Some(tray) = app_handle.tray_by_id("main-tray") {
                        let bytes = if running { ICON_RUNNING } else { ICON_STOPPED };
                        if let Ok(icon) = tauri::image::Image::from_bytes(bytes) {
                            let _ = tray.set_icon(Some(icon));
                            // Re-assert template mode after each icon swap — macOS
                            // may reset the flag when the image object is replaced.
                            let _ = tray.set_icon_as_template(true);
                        }
                    }

                    // Enable the applicable action; disable the inapplicable one.
                    #[cfg(target_os = "macos")]
                    {
                        let _ = vm_start_task.set_enabled(!running);
                        let _ = vm_stop_task.set_enabled(running);
                    }

                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            // On close: prevent the default close, then hide asynchronously.
            // We must prevent_close() first (synchronous), then spawn the hide
            // so we're not calling hide() inside the event callback — on Wayland
            // calling hide() synchronously inside CloseRequested can panic.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.hide();
                    }
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(target_os = "linux")]
fn make_backend() -> Arc<dyn backend::RuntimeBackend> {
    match backend::process::ProcessBackend::new() {
        Ok(b) => Arc::new(b),
        Err(e) => {
            log::warn!("pelagos not found in PATH: {e} — UI will show error state");
            Arc::new(backend::process::ProcessBackend::unavailable())
        }
    }
}

#[cfg(target_os = "macos")]
fn make_backend() -> Arc<dyn backend::RuntimeBackend> {
    Arc::new(backend::vsock::VsockBackend::with_default_path())
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// Locate the `pelagos` host binary (installed by the pelagos-mac formula).
///
/// macOS GUI apps launch with a stripped PATH that excludes Homebrew.
/// We check known locations before falling back to PATH lookup.
#[cfg(target_os = "macos")]
fn find_pelagos_bin() -> Option<std::path::PathBuf> {
    // Homebrew on Apple Silicon and Intel respectively.
    for candidate in &["/opt/homebrew/bin/pelagos", "/usr/local/bin/pelagos"] {
        let p = std::path::Path::new(candidate);
        if p.exists() {
            return Some(p.to_owned());
        }
    }
    // Fall back to PATH lookup (works in dev / shell launches).
    which::which("pelagos").ok()
}

/// Spawn `pelagos vm <sub>` (start or stop) in the background.
#[cfg(target_os = "macos")]
fn spawn_vm_cmd(sub: &'static str) {
    tauri::async_runtime::spawn(async move {
        let Some(bin) = find_pelagos_bin() else {
            log::warn!("pelagos not found — cannot {sub} VM");
            return;
        };
        match tokio::process::Command::new(bin)
            .args(["vm", sub])
            .output()
            .await
        {
            Ok(out) if out.status.success() => {
                log::info!("pelagos-mac vm {sub} succeeded");
            }
            Ok(out) => {
                log::warn!(
                    "pelagos-mac vm {sub} failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            Err(e) => {
                log::warn!("pelagos-mac vm {sub}: {e}");
            }
        }
    });
}

/// Inject GTK CSS to give the tray context menu an opaque background.
///
/// Without this, the menu inherits transparency from the user's GTK theme
/// and Hyprland compositor, making it unreadable.  The colours here are
/// catppuccin-mocha to match the Omarchy/Waybar default palette.
///
/// To customise: edit the CSS string below.
/// Background:  #1e1e2e (base)
/// Hover:       #313244 (surface0)
/// Text:        #cdd6f4 (text)
/// Separator:   #45475a (surface1)
#[cfg(target_os = "linux")]
fn inject_menu_css() {
    use gtk::prelude::CssProviderExt;

    let css = gtk::CssProvider::new();
    // language=CSS
    let styles = b"
        menu {
            background-color: #1e1e2e;
            border: 1px solid #45475a;
            padding: 4px 0;
        }
        menu > menuitem {
            background-color: transparent;
            color: #cdd6f4;
            padding: 4px 16px;
            min-height: 0;
        }
        menu > menuitem:hover {
            background-color: #313244;
            color: #cdd6f4;
        }
        menu > menuitem:disabled {
            color: #6c7086;
        }
        menu > separator {
            background-color: #45475a;
            margin: 4px 0;
            min-height: 1px;
        }
    ";

    if let Err(e) = css.load_from_data(styles) {
        log::warn!("failed to load tray menu CSS: {e}");
        return;
    }

    if let Some(screen) = gtk::gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
