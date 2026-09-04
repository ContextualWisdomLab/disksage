//! Black-box help and invalid-argument contracts for incomplete-download operational CLIs.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const MATERIALIZATION_SOURCE: &str =
    include_str!("../src/bin/disksage-incomplete-download-materialization.rs");
const RECOVERY_SOURCE: &str = include_str!("../src/bin/disksage-incomplete-download-recovery.rs");
const MATERIALIZE_SOURCE: &str =
    include_str!("../src/bin/disksage-incomplete-download-materialize.rs");

const BINARIES: [(&str, &str, &str); 3] = [
    (
        "disksage-incomplete-download-materialization",
        "usage: disksage-incomplete-download-materialization --root ABSOLUTE_PATH [--max-entries 1..=200000] [--stale-after-days 1..=3650] [--private-output ABSOLUTE_NEW_FILE.json]\n다음 단계: 생성된 계획을 검토하세요. 이 명령은 파일을 이동하거나 삭제하지 않습니다.",
        "incomplete-download-materialization-unknown-argument",
    ),
    (
        "disksage-incomplete-download-recovery",
        "usage: disksage-incomplete-download-recovery --root ABSOLUTE_PATH [--max-entries 1..=200000] [--stale-after-days 1..=3650] [--private-output ABSOLUTE_NEW_FILE.json]\n다음 단계: 생성된 복구 계획을 검토하세요. 이 명령은 파일을 이동하거나 삭제하지 않습니다.",
        "incomplete-download-recovery-unknown-argument",
    ),
    (
        "disksage-incomplete-download-materialize",
        "usage: disksage-incomplete-download-materialize --source-root ABSOLUTE_PATH --destination-plan ABSOLUTE_PRIVATE_PLAN.json --confirm-plan-fingerprint HEX64 --receipt-dir ABSOLUTE_PRIVATE_DIRECTORY --approved-by human:ID --rationale TEXT --execute (--live-icloud-capacity | --capacity-snapshot ABSOLUTE.json) [--max-entries 1..=200000] [--stale-after-days 1..=3650]\n다음 단계: 계획 지문과 용량 증거를 검토한 뒤 승인 정보와 --execute를 제공하세요.",
        "incomplete-download-materialize-unknown-argument",
    ),
];

fn build_feature_gated_binaries() -> (tempfile::TempDir, Vec<PathBuf>) {
    let target_dir = tempfile::tempdir().expect("isolated Cargo target directory must be created");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    command.current_dir(env!("CARGO_MANIFEST_DIR")).args([
        "build",
        "--locked",
        "--features",
        "cloud-cli",
    ]);
    for (binary, _, _) in BINARIES {
        command.args(["--bin", binary]);
    }
    let status = command
        .arg("--target-dir")
        .arg(target_dir.path())
        .status()
        .expect("feature-gated incomplete-download CLIs must be buildable for process contracts");
    assert!(
        status.success(),
        "feature-gated incomplete-download CLI build must succeed before process assertions"
    );

    let binaries = BINARIES
        .iter()
        .map(|(binary, _, _)| {
            let path = target_dir
                .path()
                .join("debug")
                .join(format!("{binary}{}", std::env::consts::EXE_SUFFIX));
            assert!(
                path.is_file(),
                "{binary} must exist after the explicit cloud-cli build"
            );
            path
        })
        .collect();
    (target_dir, binaries)
}

fn command(binary: &Path) -> Command {
    let mut command = Command::new(binary);
    command.env_remove("HOME").env_remove("USERPROFILE");
    command
}

fn assert_help_success(binary: &Path, usage: &str, flag: &str) {
    let output = command(binary)
        .arg(flag)
        .output()
        .expect("incomplete-download CLI must launch for its help contract");

    assert!(
        output.status.success(),
        "{flag} must be a successful terminal action, got status {:?} and stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "successful help must not be projected through stderr"
    );
    let stdout = String::from_utf8(output.stdout).expect("help output must be valid UTF-8");
    assert_eq!(
        stdout,
        format!("{usage}\n"),
        "help output must equal the stable usage synopsis"
    );
}

fn assert_invalid_argument_is_bounded(binary: &Path, error_token: &str) {
    let output = command(binary)
        .arg("--opaque-option=not-shown")
        .output()
        .expect("incomplete-download CLI must launch for invalid argument validation");

    assert!(
        !output.status.success(),
        "an unknown argument must remain a non-zero failure"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid invocation must not emit successful output on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(
        stderr.contains(error_token),
        "invalid invocation must use its stable error token"
    );
    assert!(
        !stderr.contains("not-shown"),
        "invalid diagnostics must not echo arbitrary argument payloads"
    );
}

fn assert_help_does_not_hide_invalid_argument(binary: &Path, error_token: &str) {
    let output = command(binary)
        .args(["--help", "--opaque-option=not-shown"])
        .output()
        .expect("incomplete-download CLI must launch for mixed help validation");

    assert!(
        !output.status.success(),
        "help must not turn an otherwise invalid invocation into success"
    );
    assert!(
        output.stdout.is_empty(),
        "mixed invalid invocation must not emit successful help on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(
        stderr.contains(error_token),
        "mixed invalid invocation must use its stable error token"
    );
    assert!(
        !stderr.contains("not-shown"),
        "mixed invalid diagnostics must not echo arbitrary argument payloads"
    );
}

#[cfg(unix)]
fn assert_non_utf8_argument_is_bounded(binary: &Path, error_token: &str) {
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', b'u', b'e', 0xff]);
    let output = command(binary)
        .arg(opaque)
        .output()
        .expect("incomplete-download CLI must launch for non-UTF-8 argument validation");

    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid non-UTF-8 input must use the ordinary bounded argument-error exit"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid non-UTF-8 input must not emit successful output"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8");
    assert!(
        stderr.contains(error_token),
        "invalid non-UTF-8 input must use its stable error token"
    );
    assert!(
        !stderr.contains("panicked") && !stderr.contains("thread 'main'"),
        "invalid host arguments must not escape through a Rust panic"
    );
}

#[cfg(unix)]
fn assert_native_non_utf8_paths_reach_domain_boundaries(binaries: &[PathBuf]) {
    use std::os::unix::ffi::OsStringExt;

    let parent = tempfile::tempdir().expect("native-path parent fixture must be created");
    let native_root = parent.path().join(OsString::from_vec(vec![
        b'i', b'n', b'c', b'o', b'm', b'p', b'l', b'e', b't', b'e', b'-', 0xff,
    ]));
    if let Err(error) = std::fs::create_dir(&native_root) {
        #[cfg(target_os = "macos")]
        if error.raw_os_error() == Some(libc::EILSEQ) {
            // APFS rejects this byte under the active locale; Linux CI exercises the
            // lossless native-path boundary while macOS keeps the unsupported case explicit.
            return;
        }
        panic!("native non-UTF-8 source root must be created: {error}");
    }

    for binary in &binaries[..2] {
        let output = command(binary)
            .arg("--root")
            .arg(&native_root)
            .output()
            .expect("read-only incomplete-download CLI must launch with a native root");
        let stderr = String::from_utf8(output.stderr)
            .expect("native-path diagnostics must remain valid UTF-8");
        if output.status.success() {
            assert!(
                stderr.is_empty(),
                "successful native-path planning must not emit diagnostics"
            );
            let summary: serde_json::Value = serde_json::from_slice(&output.stdout)
                .expect("successful native-path planning must emit machine-readable JSON");
            assert!(
                summary.is_object(),
                "planning output must remain a JSON object"
            );
        } else {
            assert_eq!(output.status.code(), Some(2));
            assert!(
                stderr.contains("materialization-unit-set-empty-or-duplicate"),
                "native path must reach the bounded materialization domain error, not argument parsing: {stderr}"
            );
            assert!(output.stdout.is_empty());
        }
        assert!(!stderr.contains("incomplete-download-materialization-unknown-argument"));
    }

    let missing_plan = parent.path().join(OsString::from_vec(vec![
        b'p', b'l', b'a', b'n', b'-', 0xff, b'.', b'j', b's', b'o', b'n',
    ]));
    let source_root = tempfile::tempdir().expect("materialization source fixture must be created");
    let receipt_dir = tempfile::tempdir().expect("receipt directory fixture must be created");
    let capacity_snapshot = parent.path().join("capacity.json");
    let output = command(&binaries[2])
        .arg("--source-root")
        .arg(source_root.path())
        .arg("--destination-plan")
        .arg(&missing_plan)
        .arg("--confirm-plan-fingerprint")
        .arg("a".repeat(64))
        .arg("--receipt-dir")
        .arg(receipt_dir.path())
        .arg("--approved-by")
        .arg("human:test")
        .arg("--rationale")
        .arg("native path admission")
        .arg("--execute")
        .arg("--capacity-snapshot")
        .arg(&capacity_snapshot)
        .output()
        .expect("materialization execution CLI must launch with a native plan path");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr =
        String::from_utf8(output.stderr).expect("materialization diagnostic must be UTF-8");
    assert!(
        stderr.contains("materialization-execution-destination-plan-unavailable"),
        "native plan path must reach bounded file admission instead of argument decoding: {stderr}"
    );
    assert!(!stderr.contains("incomplete-download-materialize-unknown-argument"));
}

fn assert_read_only_duplicate_limit_is_bounded(binary: &Path, flag: &str) {
    let root = tempfile::tempdir().expect("duplicate-limit root fixture must be created");
    let output = command(binary)
        .arg("--root")
        .arg(root.path())
        .arg(flag)
        .arg("1")
        .arg(flag)
        .arg("2")
        .output()
        .expect("read-only incomplete-download CLI must launch for duplicate-limit validation");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr =
        String::from_utf8(output.stderr).expect("duplicate-limit diagnostic must be UTF-8");
    assert!(
        stderr.contains(&format!("{flag}는 한 번만 지정할 수 있음")),
        "duplicate bounded limit must fail before domain work: {stderr}"
    );
}

fn assert_materialize_duplicate_limit_is_bounded(binary: &Path, flag: &str) {
    let source_root = tempfile::tempdir().expect("duplicate-limit source fixture must be created");
    let private = tempfile::tempdir().expect("duplicate-limit private fixture must be created");
    let output = command(binary)
        .arg("--source-root")
        .arg(source_root.path())
        .arg("--destination-plan")
        .arg(private.path().join("missing-plan.json"))
        .arg("--confirm-plan-fingerprint")
        .arg("a".repeat(64))
        .arg("--receipt-dir")
        .arg(private.path().join("receipts"))
        .arg("--approved-by")
        .arg("human:test")
        .arg("--rationale")
        .arg("duplicate limit admission")
        .arg(flag)
        .arg("1")
        .arg(flag)
        .arg("2")
        .arg("--execute")
        .arg("--capacity-snapshot")
        .arg(private.path().join("capacity.json"))
        .output()
        .expect("materialize CLI must launch for duplicate-limit validation");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr =
        String::from_utf8(output.stderr).expect("duplicate-limit diagnostic must be UTF-8");
    assert!(
        stderr.contains(&format!("{flag}는 한 번만 지정할 수 있음")),
        "duplicate bounded limit must fail before plan or filesystem admission: {stderr}"
    );
}

#[test]
fn incomplete_download_coverage_contract_keeps_shipped_entrypoints_real() {
    for (name, source) in [
        (
            "disksage-incomplete-download-materialization",
            MATERIALIZATION_SOURCE,
        ),
        ("disksage-incomplete-download-recovery", RECOVERY_SOURCE),
        (
            "disksage-incomplete-download-materialize",
            MATERIALIZE_SOURCE,
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
        assert!(
            !source.contains("#[cfg(not(coverage))]\nfn run()"),
            "the shipped {name} runtime must remain present under instrumentation"
        );
    }
}

#[test]
fn incomplete_download_help_is_successful_and_invalid_arguments_are_bounded() {
    let (_target_dir, binaries) = build_feature_gated_binaries();
    for ((_, usage, error_token), binary) in BINARIES.iter().zip(&binaries) {
        assert_help_success(binary, usage, "--help");
        assert_help_success(binary, usage, "-h");
        assert_invalid_argument_is_bounded(binary, error_token);
        assert_help_does_not_hide_invalid_argument(binary, error_token);
        #[cfg(unix)]
        assert_non_utf8_argument_is_bounded(binary, error_token);
    }
    for binary in &binaries[..2] {
        assert_read_only_duplicate_limit_is_bounded(binary, "--max-entries");
        assert_read_only_duplicate_limit_is_bounded(binary, "--stale-after-days");
    }
    assert_materialize_duplicate_limit_is_bounded(&binaries[2], "--max-entries");
    assert_materialize_duplicate_limit_is_bounded(&binaries[2], "--stale-after-days");
    #[cfg(unix)]
    assert_native_non_utf8_paths_reach_domain_boundaries(&binaries);
}
