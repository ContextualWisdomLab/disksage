//! Source-controlled inventory of every shipped binary and its help contract.
//!
//! Issue #210 requires a complete inventory that classifies each shipped binary as a successful
//! interactive-help command or an intentionally non-interactive program before any per-binary help
//! evidence is trusted. The classification below is the canonical record; enumerating the real
//! manifest through `cargo metadata` keeps it honest, so a newly added binary cannot ship without
//! being classified here. Per-binary process-level help assertions live in their own `*_help_exit`
//! tests; this file owns completeness, not behavior.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelpContract {
    /// A terminal command that must print stable usage to stdout, emit no stderr, and exit 0 for
    /// `--help`/`-h` while keeping mixed help/invalid input a bounded non-zero failure.
    Interactive,
    /// The desktop GUI bootstrap has no terminal help contract and is never launched by help smoke
    /// tests.
    DesktopGui,
}

/// Every `[[bin]]` the `disksage` crate ships, with its terminal help contract.
///
/// Keep this list sorted by target name. Adding a `[[bin]]` without a row here fails
/// `every_shipped_binary_is_classified`.
const INVENTORY: &[(&str, HelpContract)] = &[
    ("disksage", HelpContract::DesktopGui),
    ("disksage-archive-tree", HelpContract::Interactive),
    ("disksage-cache-cleanup", HelpContract::Interactive),
    (
        "disksage-cloud-local-eviction-batch",
        HelpContract::Interactive,
    ),
    ("disksage-cloud-local-inventory", HelpContract::Interactive),
    ("disksage-cloud-plan", HelpContract::Interactive),
    ("disksage-container-orphan-plan", HelpContract::Interactive),
    ("disksage-dev-artifacts", HelpContract::Interactive),
    ("disksage-duplicate-audit", HelpContract::Interactive),
    ("disksage-git-clone-reclaim", HelpContract::Interactive),
    ("disksage-git-worktree-audit", HelpContract::Interactive),
    (
        "disksage-git-worktree-metadata-prune",
        HelpContract::Interactive,
    ),
    ("disksage-git-worktree-remove", HelpContract::Interactive),
    ("disksage-icloud-local-eviction", HelpContract::Interactive),
    (
        "disksage-icloud-local-eviction-batch",
        HelpContract::Interactive,
    ),
    (
        "disksage-icloud-provider-recovery",
        HelpContract::Interactive,
    ),
    ("disksage-icloud-sync-health", HelpContract::Interactive),
    (
        "disksage-incomplete-download-audit",
        HelpContract::Interactive,
    ),
    (
        "disksage-incomplete-download-destination-plan",
        HelpContract::Interactive,
    ),
    (
        "disksage-incomplete-download-materialization",
        HelpContract::Interactive,
    ),
    (
        "disksage-incomplete-download-materialize",
        HelpContract::Interactive,
    ),
    (
        "disksage-incomplete-download-recovery",
        HelpContract::Interactive,
    ),
    ("disksage-maven-cache-audit", HelpContract::Interactive),
    ("disksage-maven-cache-prune", HelpContract::Interactive),
    (
        "disksage-multipart-archive-audit",
        HelpContract::Interactive,
    ),
    (
        "disksage-naruon-copy-readiness-verify",
        HelpContract::Interactive,
    ),
    ("disksage-podman-reclaim-plan", HelpContract::Interactive),
    ("disksage-protect-path", HelpContract::Interactive),
    (
        "disksage-provider-client-runtime",
        HelpContract::Interactive,
    ),
    ("disksage-provider-oauth", HelpContract::Interactive),
    ("disksage-provider-recovery", HelpContract::Interactive),
    ("disksage-reclaim-plan", HelpContract::Interactive),
    ("disksage-runtime-storage", HelpContract::Interactive),
    ("disksage-volume-snapshot", HelpContract::Interactive),
    ("disksage-zotero-local", HelpContract::Interactive),
];

/// Enumerates the authoritative `[[bin]]` set through Cargo itself rather than by scraping TOML, so
/// comments, strings, or duplicate tables cannot masquerade as shipped targets.
fn shipped_binaries() -> BTreeSet<String> {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--no-deps")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("cargo metadata must execute for the shipped-binary inventory");

    assert!(
        output.status.success(),
        "cargo metadata must parse the manifest successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata must emit valid JSON");
    let canonical_manifest =
        std::fs::canonicalize(&manifest_path).expect("manifest path must canonicalize");
    let package = metadata["packages"]
        .as_array()
        .expect("cargo metadata packages must be an array")
        .iter()
        .find(|package| {
            package["manifest_path"]
                .as_str()
                .map(Path::new)
                .and_then(|path| std::fs::canonicalize(path).ok())
                .is_some_and(|path| path == canonical_manifest)
        })
        .expect("cargo metadata must contain the disksage package");

    package["targets"]
        .as_array()
        .expect("package targets must be an array")
        .iter()
        .filter(|target| {
            target["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
        })
        .map(|target| {
            target["name"]
                .as_str()
                .expect("binary target names must be strings")
                .to_string()
        })
        .collect()
}

#[test]
fn every_shipped_binary_is_classified() {
    let shipped = shipped_binaries();
    let classified: BTreeSet<String> = INVENTORY
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();

    let unclassified: Vec<_> = shipped.difference(&classified).collect();
    assert!(
        unclassified.is_empty(),
        "every shipped binary must be classified in INVENTORY; unclassified: {unclassified:?}"
    );

    let stale: Vec<_> = classified.difference(&shipped).collect();
    assert!(
        stale.is_empty(),
        "INVENTORY lists targets that are no longer shipped: {stale:?}"
    );
}

#[test]
fn inventory_is_unique_and_sorted() {
    let mut previous: Option<&str> = None;
    let mut seen = BTreeSet::new();
    for (name, _) in INVENTORY {
        assert!(seen.insert(*name), "duplicate inventory entry: {name}");
        if let Some(previous) = previous {
            assert!(
                previous < *name,
                "INVENTORY must be sorted: {previous} then {name}"
            );
        }
        previous = Some(name);
    }
}

#[test]
fn every_interactive_target_names_a_process_help_test() {
    // Process-level help evidence is required by issue #210; a parser unit test does not satisfy it.
    // Each interactive target with a dedicated `*_help_exit` file is checked by content, and every
    // other target is asserted to still be classified so this list cannot silently shrink.
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let help_tests: BTreeSet<String> = std::fs::read_dir(&tests_dir)
        .expect("tests directory must be readable")
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with("_help_exit.rs"))
        .collect();

    // These interactive CLIs launch their own binary from a dedicated process help test.
    let covered = [
        "disksage-archive-tree",
        "disksage-dev-artifacts",
        "disksage-duplicate-audit",
        "disksage-git-worktree-audit",
        "disksage-git-worktree-metadata-prune",
        "disksage-git-worktree-remove",
        "disksage-icloud-provider-recovery",
        "disksage-icloud-sync-health",
        "disksage-maven-cache-audit",
        "disksage-protect-path",
        "disksage-provider-recovery",
        "disksage-runtime-storage",
        "disksage-volume-snapshot",
        "disksage-zotero-local",
    ];
    assert!(
        !help_tests.is_empty(),
        "at least one *_help_exit process test must exist for interactive help evidence"
    );
    for binary in covered {
        let stem = binary.trim_start_matches("disksage-").replace('-', "_");
        assert!(
            help_tests.contains(&format!("{stem}_help_exit.rs")),
            "interactive {binary} must keep its process help test ({stem}_help_exit.rs)"
        );
        assert_eq!(
            INVENTORY
                .iter()
                .find(|(name, _)| *name == binary)
                .map(|(_, contract)| *contract),
            Some(HelpContract::Interactive),
            "{binary} must remain an interactive help target"
        );
    }
}
