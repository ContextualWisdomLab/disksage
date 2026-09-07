//! Source-owner contract for provider OAuth private-directory publication.
//!
//! OAuth persistence consumes the crate's canonical private-directory publication module. Tests must
//! not `include!` a second module instance, because that would compile a distinct copy of the owner
//! and allow unit-test behavior to drift from the production dependency boundary.

#[test]
fn provider_oauth_uses_canonical_private_directory_publication_owner() {
    let source = include_str!("../src/provider_oauth.rs");

    assert!(
        !source.contains("provider_oauth_test_private_directory_publication"),
        "provider OAuth must not compile a test-private copy of the publication owner"
    );
    assert!(
        !source.contains("/src/private_directory_publication.rs"),
        "provider OAuth must consume the crate module instead of include!-copying owner source"
    );
    assert_eq!(
        source
            .matches(
                "crate::private_directory_publication::write_private_bytes_create_new_with_parents("
            )
            .count(),
        1,
        "provider OAuth must have one canonical private-directory publication call site"
    );
}
