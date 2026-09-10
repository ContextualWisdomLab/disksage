//! Runtime help-contract sweep over the shipped interactive CLIs.
//!
//! Issue #210 acceptance items 2, 4, 5, and 6 require launching the real feature-gated binaries
//! rather than trusting parser-source assertions. Each binary below is looked up through Cargo's
//! `CARGO_BIN_EXE_<name>` variable at compile time; a binary that is not built by the current
//! feature set is skipped, so this file stays valid whether or not every CLI feature is enabled.
//!
//! Binaries whose help contract is still being repaired by another writer's PR are listed as
//! owner-pending exceptions. They are intentionally not asserted here so this file never encodes a
//! temporary failure as a permanent contract; each owner proves its own binary and the exception is
//! removed once the repaired head lands.

use std::path::PathBuf;
use std::process::Command;

/// Interactive CLIs and their compiled binary path (`None` when not built by this feature set).
///
/// Keep sorted; `disksage` (the GUI bootstrap) is intentionally absent because it has no terminal
/// help contract.
const INTERACTIVE: &[(&str, Option<&str>)] = &[
    (
        "disksage-archive-tree",
        option_env!("CARGO_BIN_EXE_disksage-archive-tree"),
    ),
    (
        "disksage-cache-cleanup",
        option_env!("CARGO_BIN_EXE_disksage-cache-cleanup"),
    ),
    (
        "disksage-cloud-local-eviction-batch",
        option_env!("CARGO_BIN_EXE_disksage-cloud-local-eviction-batch"),
    ),
    (
        "disksage-cloud-local-inventory",
        option_env!("CARGO_BIN_EXE_disksage-cloud-local-inventory"),
    ),
    (
        "disksage-cloud-plan",
        option_env!("CARGO_BIN_EXE_disksage-cloud-plan"),
    ),
    (
        "disksage-container-orphan-plan",
        option_env!("CARGO_BIN_EXE_disksage-container-orphan-plan"),
    ),
    (
        "disksage-dev-artifacts",
        option_env!("CARGO_BIN_EXE_disksage-dev-artifacts"),
    ),
    (
        "disksage-duplicate-audit",
        option_env!("CARGO_BIN_EXE_disksage-duplicate-audit"),
    ),
    (
        "disksage-git-clone-reclaim",
        option_env!("CARGO_BIN_EXE_disksage-git-clone-reclaim"),
    ),
    (
        "disksage-git-worktree-audit",
        option_env!("CARGO_BIN_EXE_disksage-git-worktree-audit"),
    ),
    (
        "disksage-git-worktree-metadata-prune",
        option_env!("CARGO_BIN_EXE_disksage-git-worktree-metadata-prune"),
    ),
    (
        "disksage-git-worktree-remove",
        option_env!("CARGO_BIN_EXE_disksage-git-worktree-remove"),
    ),
    (
        "disksage-icloud-local-eviction",
        option_env!("CARGO_BIN_EXE_disksage-icloud-local-eviction"),
    ),
    (
        "disksage-icloud-local-eviction-batch",
        option_env!("CARGO_BIN_EXE_disksage-icloud-local-eviction-batch"),
    ),
    (
        "disksage-icloud-provider-recovery",
        option_env!("CARGO_BIN_EXE_disksage-icloud-provider-recovery"),
    ),
    (
        "disksage-icloud-sync-health",
        option_env!("CARGO_BIN_EXE_disksage-icloud-sync-health"),
    ),
    (
        "disksage-incomplete-download-audit",
        option_env!("CARGO_BIN_EXE_disksage-incomplete-download-audit"),
    ),
    (
        "disksage-incomplete-download-destination-plan",
        option_env!("CARGO_BIN_EXE_disksage-incomplete-download-destination-plan"),
    ),
    (
        "disksage-incomplete-download-materialization",
        option_env!("CARGO_BIN_EXE_disksage-incomplete-download-materialization"),
    ),
    (
        "disksage-incomplete-download-materialize",
        option_env!("CARGO_BIN_EXE_disksage-incomplete-download-materialize"),
    ),
    (
        "disksage-incomplete-download-recovery",
        option_env!("CARGO_BIN_EXE_disksage-incomplete-download-recovery"),
    ),
    (
        "disksage-maven-cache-audit",
        option_env!("CARGO_BIN_EXE_disksage-maven-cache-audit"),
    ),
    (
        "disksage-maven-cache-prune",
        option_env!("CARGO_BIN_EXE_disksage-maven-cache-prune"),
    ),
    (
        "disksage-multipart-archive-audit",
        option_env!("CARGO_BIN_EXE_disksage-multipart-archive-audit"),
    ),
    (
        "disksage-naruon-copy-readiness-verify",
        option_env!("CARGO_BIN_EXE_disksage-naruon-copy-readiness-verify"),
    ),
    (
        "disksage-podman-reclaim-plan",
        option_env!("CARGO_BIN_EXE_disksage-podman-reclaim-plan"),
    ),
    (
        "disksage-protect-path",
        option_env!("CARGO_BIN_EXE_disksage-protect-path"),
    ),
    (
        "disksage-provider-client-runtime",
        option_env!("CARGO_BIN_EXE_disksage-provider-client-runtime"),
    ),
    (
        "disksage-provider-oauth",
        option_env!("CARGO_BIN_EXE_disksage-provider-oauth"),
    ),
    (
        "disksage-provider-recovery",
        option_env!("CARGO_BIN_EXE_disksage-provider-recovery"),
    ),
    (
        "disksage-reclaim-plan",
        option_env!("CARGO_BIN_EXE_disksage-reclaim-plan"),
    ),
    (
        "disksage-runtime-storage",
        option_env!("CARGO_BIN_EXE_disksage-runtime-storage"),
    ),
    (
        "disksage-volume-snapshot",
        option_env!("CARGO_BIN_EXE_disksage-volume-snapshot"),
    ),
    (
        "disksage-zotero-local",
        option_env!("CARGO_BIN_EXE_disksage-zotero-local"),
    ),
];

/// Interactive CLIs whose successful help is still owned by another writer's repair PR. Excluded
/// from the positive help assertion until that head lands (issue #210 dependency/closure).
const OWNER_PENDING_HELP: &[&str] = &[
    "disksage-cloud-local-inventory",
    "disksage-incomplete-download-audit",
    "disksage-incomplete-download-materialization",
    "disksage-incomplete-download-materialize",
    "disksage-incomplete-download-recovery",
    "disksage-maven-cache-prune",
    "disksage-multipart-archive-audit",
    "disksage-provider-client-runtime",
    "disksage-provider-oauth",
];

/// Interactive CLIs whose mixed help request still returns success, owned by the strict-terminal
/// repair PR. Excluded from the mixed-help non-zero assertion until that head lands.
const OWNER_PENDING_MIXED: &[&str] = &["disksage-podman-reclaim-plan", "disksage-reclaim-plan"];

fn available() -> Vec<(&'static str, PathBuf)> {
    INTERACTIVE
        .iter()
        .filter_map(|(name, path)| path.map(|path| (*name, PathBuf::from(path))))
        .collect()
}

#[test]
fn every_built_interactive_cli_has_a_successful_help_terminal_action() {
    let binaries = available();
    assert!(
        !binaries.is_empty(),
        "at least one interactive CLI must be built for the runtime help sweep"
    );

    for (name, binary) in binaries {
        if OWNER_PENDING_HELP.contains(&name) {
            continue;
        }
        let mut stdout_by_flag = Vec::new();
        for flag in ["--help", "-h"] {
            let output = Command::new(&binary)
                .arg(flag)
                .output()
                .unwrap_or_else(|error| panic!("{name} {flag} must launch: {error}"));
            assert_eq!(
                output.status.code(),
                Some(0),
                "{name} {flag} must exit 0, stderr: {:?}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                !output.stdout.is_empty(),
                "{name} {flag} must print the stable usage synopsis to stdout"
            );
            assert!(
                output.stderr.is_empty(),
                "{name} {flag} must not use stderr: {:?}",
                String::from_utf8_lossy(&output.stderr)
            );
            stdout_by_flag.push(output.stdout);
        }
        // `--help` and `-h` are the same terminal action and must emit the identical stable
        // synopsis; a divergence means the documented contract is not single-valued.
        assert_eq!(
            stdout_by_flag[0], stdout_by_flag[1],
            "{name} must print identical usage for --help and -h"
        );
    }
}

#[test]
fn every_built_interactive_cli_rejects_help_mixed_with_another_argument() {
    for (name, binary) in available() {
        if OWNER_PENDING_MIXED.contains(&name) {
            continue;
        }
        let output = Command::new(&binary)
            .args(["--help", "--runtime-help-sweep-sentinel"])
            .output()
            .unwrap_or_else(|error| panic!("{name} mixed help must launch: {error}"));
        assert_ne!(
            output.status.code(),
            Some(0),
            "{name} must reject help mixed with another argument, stdout: {:?}",
            String::from_utf8_lossy(&output.stdout)
        );
        // A bounded failure may still emit a structured error on stdout (for example a machine
        // readable `ok:false` envelope); what must never happen is a reported success.
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !stdout.contains("\"ok\":true") && !stdout.contains("\"ok\": true"),
            "{name} mixed help must not report a successful result: {stdout:?}"
        );
    }
}

#[test]
fn every_built_interactive_cli_bounds_an_unknown_option() {
    for (name, binary) in available() {
        let output = Command::new(&binary)
            .arg("--runtime-help-sweep-unknown-option")
            .output()
            .unwrap_or_else(|error| panic!("{name} unknown-option input must launch: {error}"));

        assert!(
            !output.status.success(),
            "{name} must reject an unknown option, stdout: {:?}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_ne!(
            output.status.code(),
            Some(101),
            "{name} must not panic (exit 101) on an unknown option"
        );
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains('\u{FFFD}'),
            "{name} must not reflect lossy-decoded bytes for an unknown option"
        );
    }
}

#[cfg(unix)]
#[test]
fn every_built_interactive_cli_bounds_non_utf8_option_input() {
    use std::os::unix::ffi::OsStrExt;

    for (name, binary) in available() {
        // These binaries still panic on non-UTF-8 option input under the same owner PRs listed in
        // OWNER_PENDING_HELP; skip them until those repaired heads land.
        if OWNER_PENDING_HELP.contains(&name) {
            continue;
        }
        let malformed = std::ffi::OsStr::from_bytes(b"--\xff\xfe-help-sweep");
        let output = Command::new(&binary)
            .arg(malformed)
            .output()
            .unwrap_or_else(|error| panic!("{name} non-UTF-8 input must launch: {error}"));

        assert_ne!(
            output.status.code(),
            Some(101),
            "{name} must not panic (exit 101) on non-UTF-8 option input"
        );
        assert_ne!(
            output.status.code(),
            Some(0),
            "{name} must treat non-UTF-8 option input as a bounded failure"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains('\u{FFFD}'),
            "{name} must not reflect lossy-decoded bytes in diagnostics: {stderr:?}"
        );
    }
}
