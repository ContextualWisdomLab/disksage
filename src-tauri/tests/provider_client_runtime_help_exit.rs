//! Black-box process contract for the provider client-runtime audit CLI.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const EXPECTED_USAGE: &str = "usage: disksage-provider-client-runtime [--output ABSOLUTE_NEW_FILE.json]\n다음 단계: 공급자 앱 상태와 제시된 조치를 확인하세요. 이 명령은 공급자 앱을 재시작하지 않습니다.";

fn build_feature_gated_binary() -> PathBuf {
    let target_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("cloud-cli-contracts");
    std::fs::create_dir_all(&target_dir)
        .expect("shared Cargo contract target directory must be created");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let status = Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "build",
            "--locked",
            "--features",
            "cloud-cli",
            "--bin",
            "disksage-provider-client-runtime",
            "--target-dir",
        ])
        .arg(&target_dir)
        .status()
        .expect("provider client-runtime CLI must be buildable for its process contract");
    assert!(
        status.success(),
        "feature-gated provider client-runtime CLI build must succeed before process assertions"
    );

    let binary = target_dir.join("debug").join(format!(
        "disksage-provider-client-runtime{}",
        std::env::consts::EXE_SUFFIX
    ));
    assert!(
        binary.is_file(),
        "provider client-runtime binary must exist after the explicit cloud-cli build"
    );
    binary
}

fn assert_help_success(binary: &Path, flag: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg(flag)
        .output()
        .expect("provider client-runtime CLI must launch for its help contract");

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
        format!("{EXPECTED_USAGE}\n"),
        "help output must equal the stable usage synopsis"
    );
}

fn assert_invalid_argument_is_bounded(binary: &Path) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg("--opaque-option=not-shown")
        .output()
        .expect("provider client-runtime CLI must launch for invalid argument validation");

    assert!(
        !output.status.success(),
        "an unknown argument must remain a non-zero failure"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid invocation must not emit successful output on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(!stderr.is_empty(), "invalid invocation must remain visible");
    assert!(
        !stderr.contains("not-shown"),
        "invalid diagnostics must not echo arbitrary argument payloads"
    );
}

fn assert_help_does_not_hide_invalid_argument(binary: &Path) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .args(["--help", "--opaque-option=not-shown"])
        .output()
        .expect("provider client-runtime CLI must launch for mixed help validation");

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
        !stderr.is_empty(),
        "mixed invalid invocation must remain visible"
    );
    assert!(
        !stderr.contains("not-shown"),
        "mixed invalid diagnostics must not echo arbitrary argument payloads"
    );
}

fn assert_argument_failure(binary: &Path, args: &[&str], expected: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .args(args)
        .output()
        .expect("provider client-runtime CLI must launch for parser admission validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8"),
        format!("{expected}\n")
    );
}

fn assert_process_audit_is_path_free_and_output_is_create_new(binary: &Path) {
    let stdout_only = Command::new(binary)
        .env_remove("HOME")
        .output()
        .expect("provider client-runtime audit must launch without an output path");
    assert!(
        stdout_only.status.success(),
        "read-only audit must succeed even without HOME: {:?}",
        String::from_utf8_lossy(&stdout_only.stderr)
    );
    assert!(stdout_only.stderr.is_empty());
    let stdout_json: serde_json::Value = serde_json::from_slice(&stdout_only.stdout)
        .expect("provider client-runtime stdout must be JSON");
    assert_eq!(stdout_json["schema_version"], 1);
    assert_eq!(
        stdout_json["schema_kind"],
        "disksage.provider-client-runtime-audit"
    );
    assert_eq!(stdout_json["provider_count"], 3);
    assert_eq!(stdout_json["local_paths_included"], false);
    assert_eq!(stdout_json["account_identifiers_included"], false);
    assert_eq!(stdout_json["raw_process_names_included"], false);
    assert_eq!(stdout_json["remote_capacity_verified"], false);
    assert_eq!(stdout_json["remote_sync_attested"], false);
    assert_eq!(stdout_json["cloud_write_executed"], false);

    let directory = tempfile::tempdir().expect("isolated output directory must be created");
    let output_path = directory.path().join("provider-runtime-audit.json");
    let first = Command::new(binary)
        .env_remove("HOME")
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("provider client-runtime audit must launch with an output path");
    assert!(
        first.status.success(),
        "first create-new audit publication must succeed: {:?}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(first.stderr.is_empty());
    let file_bytes = std::fs::read(&output_path).expect("audit file must be published");
    let file_json: serde_json::Value =
        serde_json::from_slice(&file_bytes).expect("published audit file must be JSON");
    assert_eq!(file_json["schema_version"], 1);
    assert_eq!(file_json["local_paths_included"], false);
    assert_eq!(file_json["cloud_write_executed"], false);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&output_path)
                .expect("published audit metadata must be readable")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    let second = Command::new(binary)
        .env_remove("HOME")
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("provider client-runtime audit must launch for create-new collision validation");
    assert_eq!(second.status.code(), Some(2));
    assert!(second.stdout.is_empty());
    assert_eq!(
        String::from_utf8(second.stderr).expect("collision diagnostics must be UTF-8"),
        "provider-client-runtime-output-create-failed\n"
    );
    assert_eq!(
        std::fs::read(&output_path).expect("existing audit must remain unchanged"),
        file_bytes
    );
}

#[cfg(unix)]
fn assert_non_utf8_argument_is_bounded(binary: &Path) {
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', b'u', b'e', 0xff]);
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg(opaque)
        .output()
        .expect("provider client-runtime CLI must launch for non-UTF-8 argument validation");

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
        !stderr.is_empty(),
        "invalid non-UTF-8 input must remain visible"
    );
    assert!(
        !stderr.contains("panicked") && !stderr.contains("thread 'main'"),
        "invalid host arguments must not escape through a Rust panic"
    );
}

#[test]
fn provider_client_runtime_help_is_successful_and_invalid_arguments_are_bounded() {
    let binary = build_feature_gated_binary();
    assert_help_success(&binary, "--help");
    assert_help_success(&binary, "-h");
    assert_invalid_argument_is_bounded(&binary);
    assert_help_does_not_hide_invalid_argument(&binary);
    assert_argument_failure(
        &binary,
        &["--output"],
        "--output requires an absolute new file",
    );
    assert_argument_failure(
        &binary,
        &["--output", "relative.json"],
        "--output must be absolute",
    );

    let first = tempfile::tempdir().expect("first absolute output directory must be created");
    let second = tempfile::tempdir().expect("second absolute output directory must be created");
    let first_path = first.path().join("one.json");
    let second_path = second.path().join("two.json");
    let duplicate = Command::new(&binary)
        .args([
            OsString::from("--output"),
            first_path.clone().into_os_string(),
        ])
        .args([OsString::from("--output"), second_path.into_os_string()])
        .output()
        .expect("provider client-runtime CLI must launch for duplicate output validation");
    assert_eq!(duplicate.status.code(), Some(2));
    assert!(duplicate.stdout.is_empty());
    assert_eq!(
        String::from_utf8(duplicate.stderr).expect("duplicate diagnostics must be UTF-8"),
        "--output may be supplied once\n"
    );

    assert_process_audit_is_path_free_and_output_is_create_new(&binary);
    #[cfg(unix)]
    assert_non_utf8_argument_is_bounded(&binary);
}
