//! Haul's desktop app: Tauri glue around the engine.

pub mod bridge;
pub mod core;
pub mod dto;

pub fn run() {
    tauri::Builder::default().run(tauri::generate_context!()).expect("error while running Haul");
}
