//! Contract guard for provider-cache receipt failure cleanup.
//!
//! Once a create-new receipt has been opened, failure cleanup must act on that opened object rather
//! than unlinking the pathname. A same-user process can replace the visible name after creation; a
//! pathname unlink would then delete an unrelated replacement record.

use std::{fs, path::PathBuf};

fn reclaim_source() -> String {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(manifest.join("src/provider_cache_reclaim.rs"))
        .expect("provider-cache reclaim source must be readable")
}

#[test]
fn receipt_failure_cleanup_never_unlinks_the_visible_path() {
    let source = reclaim_source();
    let start = source
        .find("fn write_immutable_receipt_with_sealer")
        .expect("receipt writer must exist");
    let end = source[start..]
        .find("\n#[cfg(test)]\nfn restore_staged_file_without_replacement")
        .map(|offset| start + offset)
        .expect("receipt writer boundary must remain inspectable");
    let writer = &source[start..end];

    assert!(
        !writer.contains("fs::remove_file(&path)"),
        "post-create receipt failure must invalidate the exact opened record, not unlink a replaceable pathname"
    );
}
