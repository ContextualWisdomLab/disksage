use std::fs;
use std::path::PathBuf;

fn source(path: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join(path)).expect("repository source must be readable")
}

#[test]
fn generic_cleanup_route_must_fail_closed_before_path_consuming_recycle() {
    let lib = source("src/lib.rs");

    assert!(
        lib.contains("mod generic_cleanup;"),
        "the cleanup authority must live in its own fail-closed module"
    );
    assert!(
        lib.contains("generic_cleanup::clean_paths,"),
        "the Tauri clean_paths route must use the fail-closed authority"
    );
    assert!(
        !lib.contains("commands::clean_paths,"),
        "the path-consuming legacy cleanup route must not be registered"
    );
}
