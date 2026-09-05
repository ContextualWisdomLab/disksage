//! Contract guard for provider-cache receipt publication authority.
//!
//! Provider-cache must consume the inherited private-evidence create-new primitive rather than
//! reopening the final record or containing directory by pathname after admission.

use std::{fs, path::PathBuf};

fn receipt_writer_source() -> String {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(manifest.join("src/provider_cache_reclaim.rs"))
        .expect("provider-cache reclaim source must be readable")
}

#[test]
fn receipt_writer_delegates_final_record_publication_to_object_bound_foundation() {
    let source = receipt_writer_source();
    let start = source
        .find("fn write_immutable_receipt(")
        .expect("receipt writer must exist");
    let end = source[start..]
        .find("\n#[cfg(test)]\nfn restore_staged_file_without_replacement")
        .map(|offset| start + offset)
        .expect("receipt writer boundary must remain inspectable");
    let writer = &source[start..end];

    assert!(
        writer.contains("crate::private_evidence::write_object_bound_bytes_create_new"),
        "provider-cache receipt publication must consume the inherited object-bound create-new primitive"
    );
    assert!(
        !writer.contains("OpenOptions"),
        "provider-cache must not own a second pathname-based final-record open implementation"
    );
    assert!(
        !writer.contains("File::open(receipt_dir)"),
        "provider-cache must not reopen the containing directory by pathname for durability"
    );
}
