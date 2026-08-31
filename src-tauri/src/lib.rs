mod commands;
mod config;
mod error;
mod imaging;
mod llm;
mod pipeline;
mod setup;

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::Manager;

use crate::commands::AppState;
use crate::llm::LlmService;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let handle = app.handle().clone();
            // Битый или отсутствующий конфиг не должен мешать запуску.
            let cfg = config::load(&handle).unwrap_or_default();
            let resource_dir = handle.path().resource_dir().ok();

            app.manage(AppState {
                config: Mutex::new(cfg),
                llm: LlmService::new(resource_dir),
                photos: Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::backend_status,
            commands::pull_model,
            commands::add_photos,
            commands::add_photo_bytes,
            commands::remove_photo,
            commands::analyze_photos,
            commands::generate_listing,
            commands::refine_listing,
            commands::lint_listing,
            commands::setup_status,
            commands::install_ollama,
            commands::start_ollama,
        ])
        .on_window_event(|window, event| {
            // Иначе встроенный llama-server переживёт закрытие окна и продолжит
            // держать веса в видеопамяти.
            if let tauri::WindowEvent::Destroyed = event {
                let state = window.state::<AppState>();
                let llm = state.llm.clone();
                tauri::async_runtime::block_on(llm.shutdown());
            }
        })
        .run(tauri::generate_context!())
        .expect("не удалось запустить приложение");
}
