#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!("DiskSage supports only Windows, Linux, and macOS targets.");

// coverage 빌드(비-테스트)에서는 run()이 빠져 모듈 내용이 테스트에서만 쓰이므로 dead_code만 허용
pub mod archive_git_tree;
#[cfg_attr(coverage, allow(dead_code))]
mod brew_cleanup;
#[cfg_attr(coverage, allow(dead_code))]
pub mod cache_cleanup;
#[cfg_attr(coverage, allow(dead_code))]
pub mod cloud;
pub mod cloud_adr;
#[cfg(not(coverage))]
pub mod cloud_eviction;
#[path = "cloud_local_eviction.rs"]
mod cloud_local_eviction_impl;
#[path = "cloud_local_eviction_public.rs"]
pub mod cloud_local_eviction;
#[cfg(not(coverage))]
#[path = "cloud_local_eviction_batch.rs"]
mod cloud_local_eviction_batch_impl;
#[cfg(not(coverage))]
#[path = "cloud_local_eviction_batch_public.rs"]
pub mod cloud_local_eviction_batch;
pub mod cloud_local_inventory;
/// Typed backend-authored presentation contract for cloud archive plans.
pub mod cloud_plan_view;
pub mod cloud_review;
pub mod cloud_transfer;
#[cfg(not(coverage))]
mod colima_commands;
pub mod colima_platform;
#[path = "colima_reclaim.rs"]
mod colima_reclaim_impl;
#[path = "colima_reclaim_contract.rs"]
pub mod colima_reclaim;
#[cfg_attr(coverage, allow(dead_code))]
mod commands;
pub mod content_digest;
#[cfg_attr(coverage, allow(dead_code))]
mod dataset_metadata;
#[cfg_attr(coverage, allow(dead_code))]
pub mod dev_artifacts;
#[cfg_attr(coverage, allow(dead_code))]
mod dupes;
pub mod duplicate_audit;
#[cfg_attr(coverage, allow(dead_code))]
mod generic_cleanup;
pub mod git_worktree;
pub mod icloud_sync_health;
pub mod incomplete_download;
pub mod incomplete_download_materialization;
pub mod incomplete_download_materialization_destination;
pub mod incomplete_download_materialization_execution;
pub mod incomplete_download_recovery;
#[cfg_attr(coverage, allow(dead_code))]
mod inventory;
pub mod judge_calibration;
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
mod node_navigation;
#[cfg(all(test, unix))]
mod node_view_security_tests;
#[path = "onedrive_temp_reclaim.rs"]
mod onedrive_temp_reclaim_impl;
#[path = "onedrive_temp_reclaim_contract.rs"]
pub mod onedrive_temp_reclaim;
#[cfg_attr(coverage, allow(dead_code))]
mod ontology;
/// Path-free ontology organization lineage handoff for Naruon/semantic-data-portal.
pub mod organization_lineage;
#[cfg_attr(coverage, allow(dead_code))]
mod organize;
/// Bounded, path-free ontology planning for uninstalled macOS application data.
pub mod orphan;
/// Read-only evidence plus exact-identity-bound Podman reclaim execution authority.
#[path = "podman_reclaim_contract.rs"]
pub mod podman_reclaim;
pub mod private_evidence;
pub mod provider_api_client;
pub mod provider_api_write;
pub mod provider_capacity;
pub mod provider_client_runtime;
pub mod provider_evidence;
pub mod provider_global_sync;
pub mod provider_oauth;
pub mod provider_recovery;
pub mod provider_sync;
#[cfg_attr(coverage, allow(dead_code))]
mod reasoning;
/// Read-only, fail-closed logical/allocation/reclaimability evidence.
pub mod reclaim;
#[cfg_attr(coverage, allow(dead_code))]
#[path = "rules.rs"]
mod rules_impl;
#[cfg_attr(coverage, allow(dead_code))]
#[path = "rules_public.rs"]
mod rules;
#[cfg(test)]
mod rules_ontology_contract_tests;
#[cfg_attr(coverage, allow(dead_code))]
mod safety;
#[cfg_attr(coverage, allow(dead_code))]
mod scanner;
pub mod semantic_catalog;
#[cfg_attr(coverage, allow(dead_code))]
mod settings;
pub mod stale_git_clone;
#[cfg_attr(coverage, allow(dead_code))]
pub mod stale_git_clone_commands;
pub mod temp_reclaim;
#[path = "transparent_compression.rs"]
mod transparent_compression_impl;
#[path = "transparent_compression_public.rs"]
pub mod transparent_compression;
#[cfg_attr(coverage, allow(dead_code))]
mod userrules;
pub mod volume_pressure;
#[cfg_attr(coverage, allow(dead_code))]
mod web;
pub mod zotero_local;

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
            commands::cancel_cloud_copy,
            node_navigation::get_node_secure,
            commands::top_files,
            commands::list_cache_candidates,
            commands::clean_regenerable_caches,
            cache_cleanup::list_cache_targets,
            commands::list_dev_artifacts,
            generic_cleanup::fail_closed_clean_paths,
            cache_cleanup::clean_cache_contents,
            commands::clean_dev_artifacts,
            commands::recent_operations,
            commands::expand_clean_targets,
            commands::find_duplicate_files,
            commands::get_ontology,
            commands::disk_inventory,
            commands::ontology_coherence,
            commands::plan_organize,
            commands::export_organization_lineage,
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
            commands::plan_brew_cleanup,
            commands::inspect_podman_reclaim,
            colima_commands::inspect_colima_reclaim_configured,
            colima_commands::execute_colima_cache_prune_configured,
            commands::inspect_colima_dangling_images,
            commands::execute_colima_dangling_images,
            commands::inspect_colima_empty_volumes,
            commands::execute_colima_empty_volumes,
            commands::inspect_colima_guest_trim,
            commands::execute_colima_guest_trim,
            commands::execute_podman_dangling_image_prune,
            commands::judge_brew_cleanup,
            commands::validate_judge_calibration,
            commands::execute_brew_cleanup,
            commands::list_cloud_roots,
            commands::inspect_cloud_roots,
            commands::plan_icloud_local_copy_eviction,
            commands::evict_icloud_local_copy,
            commands::plan_stale_git_worktrees,
            commands::remove_stale_git_worktrees,
            commands::plan_stale_git_clone,
            stale_git_clone_commands::remove_stale_git_clone_fail_closed,
            commands::list_cloud_provider_connections,
            commands::verify_cloud_provider_capacity,
            commands::inspect_cloud_provider_client_runtime,
            commands::recover_cloud_provider_client,
            provider_recovery::cancel_finder_copy,
            commands::plan_orphan_cleanup,
            commands::clean_orphan_candidates,
            commands::inspect_icloud_new_copy_admission,
            commands::inspect_cloud_provider_global_sync,
            commands::list_cloud_review_decisions,
            commands::connect_cloud_provider,
            commands::disconnect_cloud_provider,
            commands::plan_cloud_archive,
            commands::review_cloud_candidate,
            commands::copy_cloud_candidate,
            commands::copy_cloud_candidate_via_provider_api,
            commands::adopt_existing_cloud_candidate,
            commands::attest_cloud_copy,
            commands::reconcile_cloud_receipts,
            commands::trash_verified_cloud_source
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
