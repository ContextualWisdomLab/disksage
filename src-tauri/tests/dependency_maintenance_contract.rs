//! Supply-chain maintenance regressions for direct Rust dependencies.
//!
//! These tests intentionally inspect the source-controlled manifest so an unused,
//! abandoned direct dependency cannot be reintroduced by a routine version bump.

/// DiskSage must not directly depend on the abandoned `jwalk` crate.
///
/// Upstream's 0.9.0 release explicitly marks the crate unmaintained. DiskSage has
/// no source reference to `jwalk`, so retaining it would add unsupported supply-
/// chain surface without providing product behavior.
#[test]
fn unmaintained_jwalk_is_not_a_direct_dependency() {
    let manifest = include_str!("../Cargo.toml");
    let has_jwalk_dependency = manifest.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("jwalk =") || trimmed.starts_with("jwalk=")
    });

    assert!(
        !has_jwalk_dependency,
        "remove the unused unmaintained jwalk direct dependency instead of upgrading it"
    );
}
