// coverage 빌드(비-테스트)에서는 run()이 빠져 모듈 내용이 테스트에서만 쓰이므로 dead_code만 허용
#[cfg_attr(coverage, allow(dead_code))]
mod dupes;
#[cfg_attr(coverage, allow(dead_code))]
mod commands;
#[cfg_attr(coverage, allow(dead_code))]
mod scanner;
#[cfg_attr(coverage, allow(dead_code))]
mod safety;
#[cfg_attr(coverage, allow(dead_code))]
mod rules;
#[cfg_attr(coverage, allow(dead_code))]
mod dev_artifacts;

// coverage 빌드에서 제외 — GUI 런타임은 헤드리스 테스트로 실행 불가
#[cfg(not(coverage))]
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
            commands::top_files,
            commands::list_cache_candidates,
            commands::list_dev_artifacts,
            commands::clean_paths,
            commands::recent_operations,
            commands::expand_clean_targets,
            commands::find_duplicate_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
