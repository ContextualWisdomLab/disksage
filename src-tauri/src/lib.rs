// coverage 빌드(비-테스트)에서는 run()이 빠져 모듈 내용이 테스트에서만 쓰이므로 dead_code만 허용
pub mod archive_git_tree;
#[cfg_attr(coverage, allow(dead_code))]
pub mod cloud;
#[cfg(not(coverage))]
pub mod cloud_eviction;
pub mod cloud_local_eviction;
pub mod cloud_local_eviction_batch;
pub mod cloud_local_inventory;
/// Typed backend-authored presentation contract for cloud archive plans.
pub mod cloud_plan_view;
pub mod cloud_review;
pub mod cloud_transfer;
#[cfg_attr(coverage, allow(dead_code))]
mod commands;
pub mod content_digest;
#[cfg_attr(coverage, allow(dead_code))]
mod dataset_metadata;
#[cfg_attr(coverage, allow(dead_code))]
mod dev_artifacts;
#[cfg_attr(coverage, allow(dead_code))]
mod dupes;
pub mod duplicate_audit;
pub mod git_worktree;
pub mod icloud_sync_health;
pub mod incomplete_download;
pub mod incomplete_download_materialization;
pub mod incomplete_download_materialization_destination;
pub mod incomplete_download_materialization_execution;
pub mod incomplete_download_recovery;
#[cfg_attr(coverage, allow(dead_code))]
mod inventory;
#[cfg_attr(coverage, allow(dead_code))]
mod llm;
#[cfg(all(test, target_os = "macos"))]
mod macos_temp_guard_tests;
pub mod maven_cache;
pub mod multipart_archive;
pub mod naruon_capacity;
pub mod naruon_cloud_copy_readiness;
pub mod naruon_lineage;
#[cfg_attr(coverage, allow(dead_code))]
mod ontology;
#[cfg_attr(coverage, allow(dead_code))]
mod organize;
/// Privacy-safe desktop projection of read-only Podman reclaim evidence.
pub mod podman_desktop;
/// Read-only, fail-closed Podman VM/store reclaim evidence.
pub mod podman_reclaim;
pub mod private_evidence;
pub mod provider_api_client;
pub mod provider_capacity;
pub mod provider_client_runtime;
pub mod provider_evidence;
pub mod provider_oauth;
pub mod provider_sync;
#[cfg_attr(coverage, allow(dead_code))]
mod reasoning;
/// Read-only, fail-closed logical/allocation/reclaimability evidence.
pub mod reclaim;
#[cfg_attr(coverage, allow(dead_code))]
mod rules;
#[cfg_attr(coverage, allow(dead_code))]
mod safety;
#[cfg_attr(coverage, allow(dead_code))]
mod scanner;
pub mod semantic_catalog;
#[cfg_attr(coverage, allow(dead_code))]
mod settings;
#[cfg_attr(coverage, allow(dead_code))]
mod userrules;
pub mod volume_pressure;
#[cfg_attr(coverage, allow(dead_code))]
mod web;

// coverage 빌드에서 제외 — GUI 런타임은 헤드리스 테스트로 실행 불가
#[cfg(not(coverage))]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
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
            commands::find_duplicate_files,
            commands::get_ontology,
            commands::disk_inventory,
            commands::ontology_coherence,
            commands::plan_organize,
            commands::user_rules,
            commands::execute_moves,
            commands::undo_last_moves,
            commands::model_status,
            commands::download_model,
            commands::file_verdicts,
            commands::summarize_unknown_bucket,
            commands::get_settings,
            commands::set_settings,
            commands::reason_unknown_extensions,
            commands::list_cloud_roots,
            commands::inspect_cloud_roots,
            commands::plan_icloud_local_copy_eviction,
            commands::evict_icloud_local_copy,
            commands::plan_stale_git_worktrees,
            commands::remove_stale_git_worktrees,
            commands::list_cloud_provider_connections,
            commands::verify_cloud_provider_capacity,
            commands::inspect_cloud_provider_client_runtime,
            commands::inspect_icloud_new_copy_admission,
            commands::list_cloud_review_decisions,
            commands::connect_cloud_provider,
            commands::disconnect_cloud_provider,
            commands::plan_cloud_archive,
            commands::review_cloud_candidate,
            commands::copy_cloud_candidate,
            commands::adopt_existing_cloud_candidate,
            commands::attest_cloud_copy,
            commands::trash_verified_cloud_source,
            podman_desktop::inspect_podman_reclaim,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
