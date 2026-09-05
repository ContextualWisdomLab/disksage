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
        lib.contains("provider_cache_public_boundary::execute_provider_cache_reclaim"),
        "Tauri must route provider-cache execution through the fail-closed public boundary"
    );
    assert!(
        !lib.contains("commands::execute_provider_cache_reclaim,"),
        "Tauri must not expose the lower-level provider-cache executor directly"
    );

    let unavailable = "provider-cache-identity-bound-permanent-delete-unavailable";
    let guard = boundary
        .find(unavailable)
        .expect("public boundary must reject permanent purge with a stable error");
    let delegate = boundary
        .find("crate::commands::execute_provider_cache_reclaim")
        .expect("Trash must continue through the existing evidence-bound command");
    assert!(
        guard < delegate,
        "permanent purge must fail before the lower-level executor can run"
    );

    let cli_guard = cli
        .find(unavailable)
        .expect("headless CLI must reject permanent purge with the same stable error");
    let cli_execute = cli
        .find("serde_json::to_value(execute(")
        .expect("Trash CLI execution must remain available");
    assert!(
        cli_guard < cli_execute,
        "CLI permanent-purge rejection must happen before the deletion executor"
    );
}
