//! Provider-neutral compatibility entry point for the cloud local-eviction batch CLI.
//!
//! The implementation remains single-owned by the historical iCloud-named binary source. This
//! wrapper preserves the provider-neutral CLI introduced for non-iCloud providers without mapping
//! two Cargo targets to the same source path.

#[path = "disksage-icloud-local-eviction-batch.rs"]
mod canonical_cli;

fn main() {
    canonical_cli::main();
}
