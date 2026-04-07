fn main() {
    // Embed build timestamp so the terminal banner always reflects the exact binary.
    println!(
        "cargo:rustc-env=PELAGOS_BUILD_TS={}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    tauri_build::build()
}
