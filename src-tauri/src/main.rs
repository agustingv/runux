// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Answered without opening the window.
    if matches!(std::env::args().nth(1).as_deref(), Some("--version" | "-V")) {
        println!("Runux {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    runux_lib::run()
}
