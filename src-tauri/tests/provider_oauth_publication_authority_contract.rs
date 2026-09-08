use std::fs;
use std::path::PathBuf;

fn provider_oauth_source() -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(manifest_dir.join("src/provider_oauth.rs"))
        .expect("provider_oauth.rs must remain readable to its publication contract test")
}

#[test]
fn oauth_connection_publication_uses_create_new_owner_and_refuses_existing_replacement() {
    let source = provider_oauth_source();

    assert!(
        source.contains(
            "crate::private_directory_publication::write_private_bytes_create_new_with_parents("
        ),
        "provider OAuth create-new publication must consume the canonical private-directory owner"
    );
    assert!(
        source.contains("oauth-connection-document-object-bound-replacement-unavailable"),
        "existing connection documents must fail closed while exact-source replacement authority is unavailable"
    );
    assert!(
        source.contains("oauth-connection-document-object-bound-publication-unavailable"),
        "platforms without canonical private publication must fail closed without a pathname fallback"
    );
    assert!(
        !source.contains("crate::object_bound_publication::replace_object_bound_bytes"),
        "provider OAuth must not reintroduce the superseded replacement owner while existing-record replacement is unavailable"
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
