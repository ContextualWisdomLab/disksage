//! Black-box terminal contracts for the DiskSage cloud planning CLI.
//!
//! The process boundary matters here: help must terminate before HOME/provider/filesystem work,
//! while malformed host arguments must remain bounded and non-reflective.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const USAGE: &str = "usage: disksage-cloud-plan [--list-roots | --inspect-roots] [--root PATH] [--cloud-root PATH | --provider icloud|onedrive|google-drive | --all-readable-roots --decision-summary] [--min-size-mib N] [--min-age-days N] [--limit N] [--audit-receipts --receipt-dir ABSOLUTE_PATH] [--reconcile-receipts --receipt-dir ABSOLUTE_PATH --evidence-dir ABSOLUTE_PATH [--oauth-connections ABSOLUTE_PATH [--provider-object-id GOOGLE_FILE_ID]]] [--decision-summary [--private-candidate-inspection-output ABSOLUTE_NEW_FILE.json | --review-reason-set REASON|REASON [--private-review-output ABSOLUTE_NEW_FILE.json]] | --exact-duplicate-review-prefix DIR_PREFIX --exact-duplicate-kind document|media|archive|dataset|backup|creative|incomplete-download | --export-naruon-copy-readiness --verify-capacity [--naruon-copy-readiness-output ABSOLUTE_NEW_FILE.json] | --export-semantic-catalog] [--verify-capacity [--oauth-connections ABSOLUTE_PATH] [--export-naruon-capacity]] [--capacity-reserve-mib N] [--copy-fingerprint HEX64 --receipt-dir PATH --confirm-copy-phrase EXACT --reviewed-by human:ID --review-rationale TEXT [--review-dir PATH] [--oauth-connections ABSOLUTE_PATH] | --provider-api-copy-fingerprint HEX64 --receipt-dir PATH --oauth-connections ABSOLUTE_PATH --confirm-copy-phrase EXACT --reviewed-by human:ID --review-rationale TEXT [--review-dir PATH] | --adopt-existing-fingerprint HEX64 --receipt-dir PATH --confirm-copy-phrase EXACT --reviewed-by human:ID --review-rationale TEXT [--review-dir PATH] | --attest-receipt RECEIPT.json --evidence-dir ABSOLUTE_PATH [--oauth-connections ABSOLUTE_PATH [--provider-object-id GOOGLE_FILE_ID]] | --evict-receipt RECEIPT.json --confirm-receipt-id HEX64 --eviction-dir ABSOLUTE_PATH --eviction-approval-dir ABSOLUTE_PATH --journal-path ABSOLUTE_PATH --evidence-dir ABSOLUTE_PATH --reviewed-by human:ID --review-rationale TEXT [--oauth-connections ABSOLUTE_PATH [--provider-object-id GOOGLE_FILE_ID]] | --review-candidate-fingerprint HEX64 --review-fingerprint HEX64 --review-disposition approved|held --reviewed-by human:ID --review-rationale TEXT --review-dir PATH | --export-naruon-lineage RECEIPT.json [--naruon-sync-evidence EVIDENCE.json]]";

fn build_cloud_plan() -> (tempfile::TempDir, PathBuf) {
    let target_dir = tempfile::tempdir().expect("isolated Cargo target directory must be created");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let status = Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "build",
            "--locked",
            "--features",
            "cloud-cli",
            "--bin",
            "disksage-cloud-plan",
            "--target-dir",
        ])
        .arg(target_dir.path())
        .status()
        .expect("cloud-plan CLI must be buildable for its process contract");
    assert!(status.success(), "cloud-plan CLI build must succeed before process assertions");

    let binary = target_dir
        .path()
        .join("debug")
        .join(format!("disksage-cloud-plan{}", std::env::consts::EXE_SUFFIX));
    assert!(binary.is_file(), "cloud-plan binary must exist after the explicit cloud-cli build");
    (target_dir, binary)
}

fn command(binary: &Path) -> Command {
    let mut command = Command::new(binary);
    command.env_remove("HOME").env_remove("USERPROFILE");
    command
}

fn assert_help_success(binary: &Path, flag: &str) {
    let output = command(binary)
        .arg(flag)
        .output()
        .expect("cloud-plan CLI must launch for its help contract");

    assert!(
        output.status.success(),
        "{flag} must succeed without HOME/USERPROFILE, got status {:?} and stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "successful help must not use stderr");
    assert_eq!(
        String::from_utf8(output.stdout).expect("help output must be UTF-8"),
        format!("{USAGE}\n"),
        "help must emit the exact stable synopsis plus one newline"
    );
}

fn assert_invalid_argument_is_bounded(binary: &Path, args: &[&str]) {
    let output = command(binary)
        .args(args)
        .output()
        .expect("cloud-plan CLI must launch for invalid argument validation");

    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid host arguments must use the ordinary bounded argument-error exit"
    );
    assert!(output.stdout.is_empty(), "invalid invocation must not emit success output");
    let stderr = String::from_utf8(output.stderr).expect("diagnostics must remain valid UTF-8");
    assert!(!stderr.is_empty(), "invalid invocation must remain visible");
    assert!(
        !stderr.contains("not-shown"),
        "diagnostics must not reflect an opaque argument payload"
    );
}

#[cfg(unix)]
fn assert_non_utf8_argument_is_bounded(binary: &Path) {
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', b'u', b'e', 0xff]);
    let output = command(binary)
        .arg(opaque)
        .output()
        .expect("cloud-plan CLI must launch for non-UTF-8 argument validation");

    assert_eq!(
        output.status.code(),
        Some(2),
        "non-UTF-8 option input must use the ordinary bounded error exit"
    );
    assert!(output.stdout.is_empty(), "invalid non-UTF-8 input must not emit success output");
    let stderr = String::from_utf8(output.stderr).expect("diagnostics must remain valid UTF-8");
    assert!(!stderr.is_empty(), "invalid non-UTF-8 input must remain visible");
    assert!(
        !stderr.contains("opaque") && !stderr.contains("panicked") && !stderr.contains("thread 'main'"),
        "malformed host input must neither reflect payload bytes nor escape through a Rust panic"
    );
}

#[test]
fn cloud_plan_help_is_terminal_and_invalid_host_arguments_are_bounded() {
    let (_target_dir, binary) = build_cloud_plan();

    assert_help_success(&binary, "--help");
    assert_help_success(&binary, "-h");
    assert_invalid_argument_is_bounded(&binary, &["--opaque-option=not-shown"]);
    assert_invalid_argument_is_bounded(&binary, &["--help", "--opaque-option=not-shown"]);
    #[cfg(unix)]
    assert_non_utf8_argument_is_bounded(&binary);
}
