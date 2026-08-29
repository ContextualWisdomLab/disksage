#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn write_fake_uv(path: &Path, cache: &Path, prune_body: &str) {
    fs::write(
        path,
        format!(
            "#!/bin/sh\nset -eu\nif [ \"${{1:-}}\" = '--version' ]; then printf 'uv 0.test\\n'; exit 0; fi\nif [ \"${{1:-}}\" = 'cache' ] && [ \"${{2:-}}\" = 'dir' ]; then printf '%s\\n' '{}'; exit 0; fi\nif [ \"${{1:-}}\" = 'cache' ] && [ \"${{2:-}}\" = 'prune' ]; then {}\nfi\nexit 64\n",
            cache.display(),
            prune_body
        ),
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn read_plan(binary: &str, uv: &Path) -> serde_json::Value {
    let output = Command::new(binary)
        .arg("--uv-bin")
        .arg(uv)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn execute(
    binary: &str,
    uv: &Path,
    records: &Path,
    plan: &serde_json::Value,
    rationale: &str,
) -> Output {
    Command::new(binary)
        .arg("--uv-bin")
        .arg(uv)
        .arg("--execute")
        .arg("--approved-plan-fingerprint")
        .arg(plan["plan_fingerprint"].as_str().unwrap())
        .arg("--confirm")
        .arg(plan["exact_approval_phrase"].as_str().unwrap())
        .arg("--approved-by")
        .arg("human:test")
        .arg("--rationale")
        .arg(rationale)
        .arg("--record-dir")
        .arg(records)
        .output()
        .unwrap()
}

#[test]
fn successful_prune_with_result_record_failure_still_emits_terminal_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    let records = temp.path().join("records");
    fs::create_dir(&cache).unwrap();
    fs::create_dir(&records).unwrap();
    fs::set_permissions(&records, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(cache.join("payload.whl"), b"cached").unwrap();

    let uv = temp.path().join("uv");
    write_fake_uv(
        &uv,
        &cache,
        &format!(
            "printf 'pruned\\n'; rm -rf '{}'; printf 'record sink replaced\\n' > '{}'; exit 0",
            records.display(),
            records.display()
        ),
    );

    let binary = env!("CARGO_BIN_EXE_disksage-uv-cache-reclaim");
    let plan = read_plan(binary, &uv);
    let output = execute(
        binary,
        &uv,
        &records,
        &plan,
        "preserve terminal evidence when durable result publication fails",
    );

    assert!(
        !output.status.success(),
        "audit persistence failure must remain nonzero for automation"
    );
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("a successful native mutation must still emit terminal JSON evidence");
    assert_eq!(receipt["status_code"], 0);
    assert_eq!(receipt["stdout"], "pruned\n");
    assert!(
        receipt["result_record_error"].as_str().is_some(),
        "receipt must expose a stable persistence failure code: {receipt}"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("uv-cache-reclaim-command-failed"));
}

#[test]
fn failed_native_prune_can_be_retried_with_same_plan_and_record_directory() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    let records = temp.path().join("records");
    let count_file = temp.path().join("prune-count");
    fs::create_dir(&cache).unwrap();
    fs::create_dir(&records).unwrap();
    fs::set_permissions(&records, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(cache.join("payload.whl"), b"cached").unwrap();

    let uv = temp.path().join("uv");
    write_fake_uv(
        &uv,
        &cache,
        &format!(
            "count=$(cat '{}' 2>/dev/null || printf '0'); count=$((count + 1)); printf '%s\\n' \"$count\" > '{}'; printf 'failed prune\\n' >&2; exit 7",
            count_file.display(),
            count_file.display()
        ),
    );

    let binary = env!("CARGO_BIN_EXE_disksage-uv-cache-reclaim");
    let plan = read_plan(binary, &uv);
    let first = execute(
        binary,
        &uv,
        &records,
        &plan,
        "record first failed native attempt",
    );
    assert!(!first.status.success());
    let first_receipt: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first_receipt["status_code"], 7);

    let second = execute(
        binary,
        &uv,
        &records,
        &plan,
        "retry unchanged plan after native failure",
    );
    assert!(!second.status.success());
    let second_receipt: serde_json::Value = serde_json::from_slice(&second.stdout)
        .expect("retry must reach native execution and emit a second receipt");
    assert_eq!(second_receipt["status_code"], 7);
    assert_eq!(fs::read_to_string(&count_file).unwrap().trim(), "2");

    let names = fs::read_dir(&records)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        names.iter().filter(|name| name.ends_with(".approval.json")).count(),
        2,
        "each attempt needs its own immutable approval record: {names:?}"
    );
    assert_eq!(
        names.iter().filter(|name| name.ends_with(".result.json")).count(),
        2,
        "each attempt needs its own immutable result record: {names:?}"
    );
    assert_ne!(
        first_receipt["result_record_path"], second_receipt["result_record_path"],
        "retries must not collide on the plan fingerprint filename"
    );
}
