use std::fs;
use std::path::PathBuf;

fn source(path: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join(path)).expect("repository source must be readable")
}

#[test]
fn macos_file_provider_placeholder_detection_must_not_infer_dataless_from_sparse_blocks() {
    let cloud = source("src/cloud.rs");

    if !cloud.contains("provider_placeholder_not_materialized") {
        return;
    }

    assert!(
        cloud.contains("SF_DATALESS"),
        "File Provider dataless detection must use Apple's SF_DATALESS file flag"
    );
    assert!(
        cloud.contains("st_flags()"),
        "File Provider dataless detection must inspect Darwin stat flags without opening file contents"
    );
    assert!(
        !cloud.contains("metadata.blocks() == 0"),
        "zero allocated blocks also describes ordinary sparse files and cannot prove SF_DATALESS"
    );
}
