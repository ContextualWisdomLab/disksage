//! Eviction-family singleton options must not use last-one-wins semantics.
//!
//! Exercise the shipped batch-eviction and destination-plan binaries so duplicated authority and
//! resource-limit fields fail before HOME, provider discovery, capacity reads, or filesystem work.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const BINARIES: [&str; 2] = [
    "disksage-cloud-local-eviction-batch",
    "disksage-incomplete-download-destination-plan",
];

/// Build the two shipped feature-gated CLIs once for this integration-test process.
///
/// The test harness may execute the two parser regressions concurrently. Building the full Tauri
/// `cloud-cli` graph independently in each test creates avoidable CPU/disk contention and made the
/// otherwise deterministic process contract fail under hosted CI. `OnceLock` preserves the real
/// shipped-binary boundary while making the expensive prerequisite single-writer within the test.
fn binaries() -> &'static [PathBuf] {
    static BINARY_PATHS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    BINARY_PATHS
        .get_or_init(|| {
            let target_dir = std::env::temp_dir().join(format!(
                "disksage-eviction-cli-duplicate-singletons-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&target_dir)
                .expect("isolated Cargo target directory must be created");
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
            let mut command = Command::new(cargo);
            command
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .args(["build", "--locked", "--features", "cloud-cli"]);
            for binary in BINARIES {
                command.args(["--bin", binary]);
            }
            let status = command
                .arg("--target-dir")
                .arg(&target_dir)
                .status()
                .expect("eviction-family CLIs must be buildable for process contracts");
            assert!(status.success(), "eviction-family CLI build must succeed");
            let paths = BINARIES
                .iter()
                .map(|binary| {
                    target_dir
                        .join("debug")
                        .join(format!("{binary}{}", std::env::consts::EXE_SUFFIX))
                })
                .collect::<Vec<_>>();
            for path in &paths {
                assert!(path.is_file(), "expected CLI binary must exist after build");
            }
            paths
        })
        .as_slice()
}

fn assert_rejected(binary: &Path, args: &[&OsStr], expected: &str) {
    let output = Command::new(binary)
        .args(args)
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .output()
        .expect("CLI must launch for duplicate-option validation");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "invalid input must not emit success JSON");
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must remain valid UTF-8");
    assert_eq!(stderr.trim_end(), expected);
}

#[test]
fn batch_eviction_rejects_duplicate_authority_options() {
    let binaries = binaries();
    let fixture = tempfile::tempdir().expect("fixture directory must be created");
    let root = fixture.path().join("Cloud");
    let other_root = fixture.path().join("OtherCloud");
    let manifest = fixture.path().join("manifest.json");
    let other_manifest = fixture.path().join("other.json");

    assert_rejected(
        &binaries[0],
        &[
            OsStr::new("--cloud-root"),
            root.as_os_str(),
            OsStr::new("--cloud-root"),
            other_root.as_os_str(),
            OsStr::new("--manifest"),
            manifest.as_os_str(),
        ],
        "--cloud-root는 한 번만 지정할 수 있음",
    );
    assert_rejected(
        &binaries[0],
        &[
            OsStr::new("--cloud-root"),
            root.as_os_str(),
            OsStr::new("--manifest"),
            manifest.as_os_str(),
            OsStr::new("--manifest"),
            other_manifest.as_os_str(),
        ],
        "--manifest는 한 번만 지정할 수 있음",
    );
    assert_rejected(
        &binaries[0],
        &[
            OsStr::new("--cloud-root"),
            root.as_os_str(),
            OsStr::new("--manifest"),
            manifest.as_os_str(),
            OsStr::new("--execute"),
            OsStr::new("--execute"),
        ],
        "--execute는 한 번만 지정할 수 있음",
    );
}

#[test]
fn destination_plan_rejects_duplicate_resource_limits() {
    let binaries = binaries();
    let fixture = tempfile::tempdir().expect("fixture directory must be created");
    let source = fixture.path().join("source");
    let cloud = fixture.path().join("cloud");
    let capacity = fixture.path().join("capacity.json");
    let prefix = [
        OsStr::new("--source-root"),
        source.as_os_str(),
        OsStr::new("--cloud-root"),
        cloud.as_os_str(),
        OsStr::new("--destination-subdirectory"),
        OsStr::new("Recovered"),
        OsStr::new("--capacity-snapshot"),
        capacity.as_os_str(),
    ];

    let mut max_entries = prefix.to_vec();
    max_entries.extend([
        OsStr::new("--max-entries"),
        OsStr::new("1"),
        OsStr::new("--max-entries"),
        OsStr::new("2"),
    ]);
    assert_rejected(
        &binaries[1],
        &max_entries,
        "DiskSage incomplete download destination plan: --max-entries는 한 번만 지정할 수 있음",
    );

    let mut stale = prefix.to_vec();
    stale.extend([
        OsStr::new("--stale-after-days"),
        OsStr::new("1"),
        OsStr::new("--stale-after-days"),
        OsStr::new("2"),
    ]);
    assert_rejected(
        &binaries[1],
        &stale,
        "DiskSage incomplete download destination plan: --stale-after-days는 한 번만 지정할 수 있음",
    );

    let mut reserve = prefix.to_vec();
    reserve.extend([
        OsStr::new("--capacity-reserve-mib"),
        OsStr::new("1"),
        OsStr::new("--capacity-reserve-mib"),
        OsStr::new("2"),
    ]);
    assert_rejected(
        &binaries[1],
        &reserve,
        "DiskSage incomplete download destination plan: --capacity-reserve-mib는 한 번만 지정할 수 있음",
    );
}
