use std::path::PathBuf;

use anyhow::Context;
use fluxion_tauri_bridge::CoreState;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir =
                app_data_dir(app.handle()).unwrap_or_else(fluxion_runtime::default_data_dir);
            let core = tauri::async_runtime::block_on(fluxion_runtime::build_core(data_dir))
                .context("initialize Fluxion Core")?;
            app.manage(CoreState::new(core));
            fluxion_tauri_bridge::register_events(app.handle());
            Ok(())
        })
        .invoke_handler(fluxion_tauri_bridge::generate_handler!())
        .run(tauri::generate_context!())
        .expect("failed to run Fluxion");
}

fn app_data_dir<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) -> Option<PathBuf> {
    handle
        .path()
        .app_data_dir()
        .ok()
        .map(|path| path.join("core"))
}
