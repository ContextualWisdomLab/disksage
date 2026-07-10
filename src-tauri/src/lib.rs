mod commands;
mod scanner;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_roots,
            commands::start_scan,
            commands::cancel_scan,
            commands::get_node,
            commands::top_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
