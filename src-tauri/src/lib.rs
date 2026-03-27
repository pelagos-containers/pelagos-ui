mod backend;
mod commands;

use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

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
            let menu = Menu::with_items(app, &[&open, &sep, &quit])?;

            TrayIconBuilder::new()
                .icon(tauri::image::Image::from_bytes(include_bytes!(
                    "../icons/tray.png"
                ))?)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
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
