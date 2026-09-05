use std::fs;
use std::path::PathBuf;

fn provider_oauth_source() -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(manifest_dir.join("src/provider_oauth.rs"))
        .expect("provider_oauth.rs must remain readable to its publication contract test")
}

#[test]
fn oauth_connection_publication_consumes_the_object_bound_owner() {
    let source = provider_oauth_source();

    assert!(
        source.contains("crate::object_bound_publication::replace_object_bound_bytes"),
        "provider OAuth must consume the canonical object-bound replacement primitive"
    );
    assert!(
        source.contains("oauth-connection-directory-sync-failed"),
        "containing-directory durability failure must remain a stable OAuth-domain error"
    );
    assert!(
        source.contains("oauth-connection-document-publication-uncertain"),
        "post-publication namespace drift must not be reported as a clean rollback"
    );
    assert!(
        source.contains("oauth-connection-document-object-bound-publication-unavailable"),
        "platforms without object-bound publication must fail closed without a pathname fallback"
    );

    for forbidden in [
        "std::fs::rename(&temporary, path)",
        "std::fs::remove_file(&temporary)",
        "options.open(&temporary)",
        "libc::openat(",
        "libc::renameat(",
        "libc::unlinkat(",
    ] {
        assert!(
            !source.contains(forbidden),
            "provider OAuth must not duplicate or bypass the canonical publication owner: {forbidden}"
        );
    }
}
