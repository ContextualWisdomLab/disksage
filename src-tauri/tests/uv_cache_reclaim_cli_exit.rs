#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::process::Command;

#[test]
fn nonzero_native_prune_exits_nonzero_after_emitting_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    let records = temp.path().join("records");
    fs::create_dir(&cache).unwrap();
    fs::create_dir(&records).unwrap();
    fs::set_permissions(&records, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(cache.join("payload.whl"), b"cached").unwrap();

    let uv = temp.path().join("uv");
    fs::write(
        &uv,
        format!(
            "#!/bin/sh\nset -eu\nif [ \"${{1:-}}\" = '--version' ]; then printf 'uv 0.8.0\\n'; exit 0; fi\nif [ \"${{1:-}}\" = 'cache' ] && [ \"${{2:-}}\" = 'dir' ]; then printf '%s\\n' '{}'; exit 0; fi\nif [ \"${{1:-}}\" = 'cache' ] && [ \"${{2:-}}\" = 'prune' ]; then printf 'native prune failed\\n' >&2; exit 7; fi\nexit 64\n",
            cache.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&uv, fs::Permissions::from_mode(0o700)).unwrap();

    let binary = env!("CARGO_BIN_EXE_disksage-uv-cache-reclaim");
    let plan_output = Command::new(binary)
        .args(["--uv-bin"])
        .arg(&uv)
        .output()
        .unwrap();
    assert!(
        plan_output.status.success(),
        "{}",
        String::from_utf8_lossy(&plan_output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&plan_output.stdout).unwrap();
    let fingerprint = plan["plan_fingerprint"].as_str().unwrap();
    let phrase = plan["exact_approval_phrase"].as_str().unwrap();

    let output = Command::new(binary)
        .arg("--uv-bin")
        .arg(&uv)
        .arg("--execute")
        .arg("--approved-plan-fingerprint")
        .arg(fingerprint)
        .arg("--confirm")
        .arg(phrase)
        .arg("--approved-by")
        .arg("human:test")
        .arg("--rationale")
        .arg("verify nonzero native prune propagation")
        .arg("--record-dir")
        .arg(&records)
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "failed native prune must not exit zero"
    );
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["status_code"], 7);
    assert!(receipt["execution_error"].is_null());
    assert!(String::from_utf8_lossy(&output.stderr).contains("uv-cache-reclaim-command-failed"));
}

#[test]
fn incomplete_cache_inventory_blocks_native_prune_authority() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    fs::create_dir(&cache).unwrap();
    let payload = cache.join("payload.whl");
    fs::write(&payload, b"cached").unwrap();
    symlink(&payload, cache.join("payload-link.whl")).unwrap();

    let uv = temp.path().join("uv");
    fs::write(
        &uv,
        format!(
            "#!/bin/sh\nset -eu\nif [ \"${{1:-}}\" = '--version' ]; then printf 'uv 0.8.0\\n'; exit 0; fi\nif [ \"${{1:-}}\" = 'cache' ] && [ \"${{2:-}}\" = 'dir' ]; then printf '%s\\n' '{}'; exit 0; fi\nif [ \"${{1:-}}\" = 'cache' ] && [ \"${{2:-}}\" = 'prune' ]; then printf 'unexpected prune\\n' >&2; exit 70; fi\nexit 64\n",
            cache.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&uv, fs::Permissions::from_mode(0o700)).unwrap();

    let binary = env!("CARGO_BIN_EXE_disksage-uv-cache-reclaim");
    let output = Command::new(binary)
        .args(["--uv-bin"])
        .arg(&uv)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "planning should return fail-closed evidence: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(plan["cache_entries_skipped"].as_u64().unwrap() > 0);
    let blockers = plan["blockers"].as_array().unwrap();
    assert!(
        blockers
            .iter()
            .any(|blocker| blocker == "cache-inventory-incomplete"),
        "incomplete traversal must block native prune authority: {plan}"
    );
}
