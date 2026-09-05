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
    let boundary = source("src/provider_cache_public_boundary.rs");

    assert!(
        lib.contains("provider_cache_public_boundary::plan_provider_cache_reclaim"),
        "Tauri planning must pass through the boundary that omits unavailable permanent approval"
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
        boundary.contains("use crate::provider_cache::{"),
        "the shipped boundary must serialize the commercial provider-cache DTOs"
    );
    assert!(
        boundary.contains("map(crate::provider_cache::project_plan)"),
        "shipped plans must be projected into a schema that cannot name permanent approval"
    );
    assert!(
        !boundary.contains("plan.exact_approval_phrase = None"),
        "the product contract must omit irreversible approval by type, not serialize the field as null"
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
        !public_signature.contains("mode:"),
        "the shipped Tauri command must not deserialize a caller-selected cleanup mode"
    );
    assert!(
        command_body[delegate..].contains("InternalProviderCacheCleanupMode::Trash"),
        "the shipped Tauri command must delegate with the sole commercial-safe Trash mode"
    );
    assert!(
        command_body[delegate..].contains("crate::provider_cache::project_trash_result"),
        "the shipped result must be projected back through the Trash-only commercial DTO"
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
        .find("serde_json::to_value(execute_trash(")
        .expect("Trash CLI execution must remain available through the safe public facade");
    assert!(
        cli_guard < cli_rejection && cli_rejection < cli_execute,
        "CLI permanent-purge rejection must happen before the Trash executor"
    );
}

#[test]
fn irreversible_lower_level_executor_is_not_a_public_rust_capability() {
    let lib = source("src/lib.rs");
    let public_api = source("src/provider_cache.rs");
    let cli = source("src/bin/disksage-provider-cache-reclaim.rs");

    assert!(
        lib.contains("mod provider_cache_reclaim;"),
        "historical irreversible implementation must stay crate-private repair evidence"
    );
    assert!(
        !lib.contains("pub mod provider_cache_reclaim;"),
        "direct Rust callers must not receive the historical lower-level executor"
    );
    assert!(
        lib.contains("pub mod provider_cache;"),
        "commercial Rust callers need one explicit safe provider-cache facade"
    );
    assert!(
        public_api.contains("pub fn execute_trash("),
        "the public Rust facade must expose the reversible lifecycle explicitly"
    );

    let reexport_start = public_api
        .find("pub use crate::provider_cache_reclaim::{")
        .expect("safe facade must explicitly choose the read-only and Trash DTOs it exports");
    let reexport_tail = &public_api[reexport_start..];
    let reexport_end = reexport_tail
        .find("};")
        .expect("safe facade re-export block must terminate");
    let reexport_block = &reexport_tail[..reexport_end];
    assert!(
        !reexport_block.contains("ProviderCacheCleanupMode"),
        "commercial Rust callers must not receive the historical cleanup-mode enum"
    );
    assert!(
        !reexport_block.contains("ProviderCacheCleanupResult"),
        "commercial Rust result types must not expose a field whose mode type still includes PermanentPurge"
    );
    assert!(
        !reexport_block.contains("ProviderCacheReclaimPlan"),
        "commercial Rust callers must not receive the historical plan type that names permanent approval"
    );

    let public_mode_start = public_api
        .find("pub enum ProviderCacheCleanupMode")
        .expect("safe facade must define its own Trash-only cleanup mode");
    let public_mode_tail = &public_api[public_mode_start..];
    let public_mode_end = public_mode_tail
        .find('}')
        .expect("safe facade cleanup mode must terminate");
    let public_mode = &public_mode_tail[..public_mode_end];
    assert!(
        public_mode.contains("Trash") && !public_mode.contains("PermanentPurge"),
        "the externally nameable provider-cache cleanup mode must contain Trash only"
    );
    assert!(
        public_api.contains("pub struct ProviderCacheReclaimPlan"),
        "safe facade must own a commercial plan DTO that omits irreversible approval"
    );
    assert!(
        public_api.contains("pub struct ProviderCacheCleanupResult"),
        "safe facade must project the internal result into a publicly nameable Trash-only DTO"
    );

    let execute_start = public_api
        .find("pub fn execute_trash(")
        .expect("Trash facade must remain available");
    let execute_body = &public_api[execute_start..];
    assert!(
        !execute_body.contains("ProviderCacheCleanupMode::PermanentPurge")
            && execute_body.contains("ProviderCacheCleanupMode::Trash"),
        "the public Rust facade must never delegate irreversible mode"
    );
    assert!(
        cli.contains("use disksage_lib::provider_cache::{"),
        "the shipped headless CLI must consume the safe public Rust facade"
    );
    assert!(
        !cli.contains("disksage_lib::provider_cache_reclaim"),
        "the shipped CLI must not import the crate-private historical executor"
    );
}

#[test]
fn lower_level_permanent_purge_fails_closed_before_receipt_or_mutation() {
    let implementation = source("src/provider_cache_reclaim.rs");
    let execute_start = implementation
        .find("pub fn execute(")
        .expect("historical lower-level execute boundary must remain inspectable");
    let execute_body = &implementation[execute_start..];

    let guard = execute_body
        .find("if mode == ProviderCacheCleanupMode::PermanentPurge")
        .expect("lower-level execution must reject irreversible mode explicitly");
    let rejection = execute_body[guard..]
        .find("provider-cache-identity-bound-permanent-delete-unavailable")
        .map(|offset| guard + offset)
        .expect("irreversible mode must use the stable unavailable error");
    let receipt = execute_body
        .find("write_immutable_receipt(")
        .expect("Trash execution must keep immutable approval receipts");
    let mutation = execute_body
        .find("permanently_purge_exact(&candidate")
        .expect("historical purge implementation remains repair evidence until safely retired");

    assert!(
        guard < rejection && rejection < receipt && rejection < mutation,
        "irreversible mode must fail closed before receipt creation or pathname-based permanent mutation"
    );
}
