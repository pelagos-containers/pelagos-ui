#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // GDK_SCALE=2 is set system-wide for GTK apps, but on Wayland GTK3 also
    // receives the display scale from the compositor protocol.  Applying both
    // doubles the scale, making all pointer-event coordinates land at half the
    // expected position (hover/click miss every element).  Unset it here so
    // GTK uses only the Wayland-native scale, which is correct.
    #[cfg(target_os = "linux")]
    std::env::remove_var("GDK_SCALE");

    pelagos_ui_lib::run();
}
