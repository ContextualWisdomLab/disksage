use std::fs;
use std::path::PathBuf;

fn source(path: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join(path)).expect("repository source must be readable")
}

#[test]
fn generic_cleanup_route_must_fail_closed_before_path_consuming_recycle() {
    let lib = source("src/lib.rs");
    let cleanup = source("src/generic_cleanup.rs");

    assert!(
        lib.contains("mod generic_cleanup;"),
        "the cleanup authority must live in its own fail-closed module"
    );
    assert!(
        lib.contains("generic_cleanup::fail_closed_clean_paths,"),
        "the Tauri clean_paths route must use the fail-closed authority"
    );
    assert!(
        !lib.contains("commands::clean_paths,"),
        "the path-consuming legacy cleanup route must not be registered"
    );
    assert!(
        cleanup.contains("#[tauri::command(rename = \"clean_paths\")]"),
        "the fail-closed Rust handler must preserve the stable clean_paths IPC command name"
    );
    assert!(
        cleanup.contains("pub fn fail_closed_clean_paths("),
        "the fail-closed Rust handler name must remain distinct from the legacy command macro"
    );
    assert!(
        !cleanup.contains(
            "#[cfg(not(coverage))]\n#[tauri::command(rename = \"clean_paths\")]"
        ),
        "coverage builds must retain the shipped fail-closed clean_paths handler instead of compiling a different command surface"
    );
}

#[test]
fn generic_cleanup_repair_must_not_unregister_identity_bound_dev_artifact_cleanup() {
    let lib = source("src/lib.rs");

    assert!(
        lib.contains("commands::clean_dev_artifacts,"),
        "generic clean_paths hardening must preserve the separately object-bound clean_dev_artifacts IPC route"
    );
}
