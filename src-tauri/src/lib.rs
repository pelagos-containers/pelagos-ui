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
            let open = MenuItem::with_id(app, "open", "Open Dashboard", true, None::<&str>)?;
            let sep  = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &sep, &quit])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
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
            // Hide instead of close so the tray app keeps running.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().unwrap();
                api.prevent_close();
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
