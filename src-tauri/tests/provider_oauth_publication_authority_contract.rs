use std::fs;
use std::path::PathBuf;

fn provider_oauth_source() -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(manifest_dir.join("src/provider_oauth.rs"))
        .expect("provider_oauth.rs must remain readable to its publication contract test")
}

#[test]
fn oauth_connection_publication_is_descriptor_bound_on_unix() {
    let source = provider_oauth_source();

    assert!(
        source.contains("libc::openat("),
        "connection-document temporary creation must be relative to a pinned directory descriptor"
    );
    assert!(
        source.contains("libc::renameat("),
        "connection-document replacement must remain relative to the pinned directory descriptor"
    );
    assert!(
        source.contains("libc::unlinkat("),
        "failure cleanup must not be redirected through a replaced pathname ancestor"
    );
    assert!(
        source.contains("oauth-connection-directory-sync-failed"),
        "successful replacement must distinguish file-data sync from containing-directory sync"
    );
    assert!(
        !source.contains("std::fs::rename(&temporary, path)"),
        "pathname rename reintroduces the ancestor-replacement publication race"
    );
}
