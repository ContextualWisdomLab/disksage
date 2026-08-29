//! Shipped-runtime and coverage-entrypoint contract for eviction/destination CLIs.
//!
//! Repository-wide coverage is intentionally collected without defining `cfg(coverage)` because
//! that synthetic cfg historically changed production semantics. This regression therefore builds
//! the real feature-gated binaries in their production configuration, executes terminal help, and
//! separately prevents either entrypoint from regaining a coverage-only no-op replacement.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const LOCAL_EVICTION_SOURCE: &str = include_str!("../src/bin/disksage-icloud-local-eviction.rs");
const DESTINATION_PLAN_SOURCE: &str =
    include_str!("../src/bin/disksage-incomplete-download-destination-plan.rs");

const BINARIES: [(&str, &str); 2] = [
    (
        "disksage-icloud-local-eviction",
        "usage: disksage-icloud-local-eviction --cloud-root ABSOLUTE_PATH --path ABSOLUTE_FILE [--execute --approved-plan-fingerprint HEX64 --confirm-plan-fingerprint HEX64 --approved-by human:IDENTITY --rationale TEXT --record-dir ABSOLUTE_LOCAL_DIRECTORY]",
    ),
    (
        "disksage-incomplete-download-destination-plan",
        "usage: disksage-incomplete-download-destination-plan --source-root ABSOLUTE_PATH --cloud-root ABSOLUTE_PATH --destination-subdirectory RELATIVE_PATH (--live-icloud-capacity | --capacity-snapshot ABSOLUTE.json) [--max-entries 1..=200000] [--stale-after-days 1..=3650] [--capacity-reserve-mib 0..=1048576] [--private-output ABSOLUTE_NEW_FILE.json]",
    ),
];

fn build_shipped_binary(binary: &str, target_dir: &Path) -> PathBuf {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let build = Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "build",
            "--locked",
            "--features",
            "cloud-cli",
            "--bin",
            binary,
            "--target-dir",
        ])
        .arg(target_dir)
        .output()
        .expect("Cargo should start for the shipped operational CLI");
    assert!(
        build.status.success(),
        "shipped {binary} build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    target_dir
        .join("debug")
        .join(format!("{binary}{}", std::env::consts::EXE_SUFFIX))
}

#[test]
fn eviction_coverage_contract_keeps_shipped_entrypoints_real() {
    for (name, source) in [
        ("disksage-icloud-local-eviction", LOCAL_EVICTION_SOURCE),
        (
            "disksage-incomplete-download-destination-plan",
            DESTINATION_PLAN_SOURCE,
        ),
    ] {
        assert!(
            !source.contains("#[cfg(coverage)]\nfn main()"),
            "coverage must never replace the shipped {name} entrypoint with a synthetic main"
        );
        assert!(
            !source.contains("#[cfg(not(coverage))]\nfn main()"),
            "the shipped {name} entrypoint must remain present under instrumentation"
        );
    }
}

#[test]
fn shipped_eviction_clis_preserve_terminal_help_runtime() {
    let target = tempfile::tempdir().expect("isolated build target must be created");

    for (binary, expected_usage) in BINARIES {
        let executable = build_shipped_binary(binary, target.path());
        let output = Command::new(&executable)
            .env_remove("HOME")
            .env_remove("USERPROFILE")
            .env_remove("APPDATA")
            .env_remove("XDG_DATA_HOME")
            .arg("--help")
            .output()
            .expect("shipped operational CLI must launch");

        assert_eq!(
            output.status.code(),
            Some(0),
            "shipped help must remain successful for {binary}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "successful shipped help must keep stderr empty for {binary}"
        );
        assert_eq!(
            String::from_utf8(output.stdout).expect("help output must stay valid UTF-8"),
            format!("{expected_usage}\n"),
            "the shipped help runtime must remain exact for {binary}"
        );
    }
}
