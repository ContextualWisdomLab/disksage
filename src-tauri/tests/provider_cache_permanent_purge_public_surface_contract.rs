use std::fs;
use std::path::PathBuf;

fn source(path: &str) -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(manifest_dir.join(path)).expect("public-surface source must remain readable")
}

#[test]
fn permanent_provider_cache_purge_is_not_shipped_through_tauri_or_cli() {
    let lib = source("src/lib.rs");
    let cli = source("src/bin/disksage-provider-cache-reclaim.rs");
    let boundary_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/provider_cache_public_boundary.rs");
    let boundary = fs::read_to_string(&boundary_path)
        .expect("provider-cache public boundary must own irreversible-mode admission");

    assert!(
        lib.contains("provider_cache_public_boundary::plan_provider_cache_reclaim"),
        "Tauri planning must pass through the boundary that hides unavailable permanent approval"
    );
    assert!(
        lib.contains("provider_cache_public_boundary::execute_provider_cache_reclaim"),
        "Tauri must route provider-cache execution through the fail-closed public boundary"
    );
    assert!(
        !lib.contains("commands::plan_provider_cache_reclaim,"),
        "Tauri must not expose the historical planner with an irreversible approval phrase"
    );
    assert!(
        !lib.contains("commands::execute_provider_cache_reclaim,"),
        "Tauri must not expose the lower-level provider-cache executor directly"
    );

    assert!(
        boundary.contains("plan.exact_approval_phrase = None"),
        "shipped plans must not advertise permanent deletion while that authority is unavailable"
    );

    let command_start = boundary
        .find("pub fn execute_provider_cache_reclaim(")
        .expect("Tauri provider-cache execution command must remain registered");
    let command_body = &boundary[command_start..];
    let delegate = command_body
        .find("crate::commands::execute_provider_cache_reclaim")
        .expect("Trash must continue through the existing evidence-bound command");
    let public_signature = &command_body[..delegate];
    assert!(
        !public_signature.contains("mode: ProviderCacheCleanupMode"),
        "the shipped Tauri command must not deserialize a caller-selected irreversible mode"
    );
    assert!(
        command_body[delegate..].contains("ProviderCacheCleanupMode::Trash"),
        "the shipped Tauri command must delegate with the sole commercial-safe Trash mode"
    );

    let unavailable = "provider-cache-identity-bound-permanent-delete-unavailable";
    assert!(
        cli.contains(unavailable),
        "the headless CLI must retain one stable permanent-purge rejection code"
    );
    let cli_guard = cli
        .find("if permanent_purge")
        .expect("headless CLI must branch on permanent purge before execution");
    let cli_rejection = cli[cli_guard..]
        .find("PERMANENT_PURGE_UNAVAILABLE")
        .map(|offset| cli_guard + offset)
        .expect("headless CLI must reject permanent purge with the stable error");
    let cli_execute = cli
        .find("serde_json::to_value(execute(")
        .expect("Trash CLI execution must remain available");
    assert!(
        cli_guard < cli_rejection && cli_rejection < cli_execute,
        "CLI permanent-purge rejection must happen before the deletion executor"
    );
}

#[test]
fn lower_level_executor_rejects_permanent_purge_before_replan_or_mutation_evidence() {
    let lower_level = source("src/provider_cache_reclaim.rs");
    let unavailable = "provider-cache-identity-bound-permanent-delete-unavailable";
    let execute_start = lower_level
        .find("pub fn execute(")
        .expect("lower-level provider-cache executor must remain observable while repair evidence exists");
    let execute_body = &lower_level[execute_start..];
    let guard = execute_body
        .find("mode == ProviderCacheCleanupMode::PermanentPurge")
        .expect("lower-level executor must explicitly fail closed on irreversible mode");
    let replan = execute_body
        .find("let current = plan_with_runtime")
        .expect("Trash execution must continue to re-plan before mutation");
    let receipt = execute_body
        .find("let receipt = write_immutable_receipt")
        .expect("Trash execution must retain immutable receipt evidence");

    assert!(
        guard < replan
            && guard < receipt
            && execute_body[guard..replan].contains(unavailable),
        "direct Rust callers must be refused permanent purge before re-plan, receipt publication, or filesystem mutation"
    );
}
