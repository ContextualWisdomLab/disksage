//! Source contract for exact private-record mode revalidation.
//!
//! Private publication must reject special-bit drift as well as owner/group/other permission drift.
//! Checking only `0o777` would allow setuid/setgid/sticky bits to change after the admitted staging
//! object was synced while still satisfying the nominal `0o600` contract.

const SOURCE: &str = include_str!("../src/object_bound_publication.rs");

#[test]
fn opened_and_visible_publication_modes_include_special_bits() {
    assert!(
        SOURCE.contains("opened.permissions().mode() & 0o7777 != unix_mode"),
        "opened staging mode must compare the complete permission/special-bit mask"
    );
    assert!(
        SOURCE.contains("visible.st_mode as u32 & 0o7777 != unix_mode"),
        "visible staging/final mode must compare the complete permission/special-bit mask"
    );
}
