//! Source contract for private-record replacement mutation authority.
//!
//! Revalidating a staging pathname and then calling `renameat` leaves a final namespace interval in
//! which a same-UID process can replace that pathname. A private publication primitive must either
//! mutate through an identity-bound source handle or fail closed before publication; post-rename
//! detection is not sufficient because unreviewed bytes may already have become authoritative.

const SOURCE: &str = include_str!("../src/object_bound_publication.rs");

#[test]
fn private_replacement_does_not_publish_through_raw_source_name_renameat() {
    assert!(
        !SOURCE.contains("libc::renameat("),
        "private replacement must not publish through a source-name-only rename; use an accepted identity-bound primitive or fail closed"
    );
}
