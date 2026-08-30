#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

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

#[test]
fn open_cached_payload_blocks_native_prune_without_global_lock() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    fs::create_dir(&cache).unwrap();
    let payload = cache.join("tool.bin");
    fs::write(&payload, b"cached executable payload").unwrap();
    let _open_payload = fs::File::open(&payload).unwrap();

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
        "planning should return active-use evidence: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["active_use"]["active"], true);
    assert_eq!(plan["active_use"]["evidence_complete"], true);
    let blockers = plan["blockers"].as_array().unwrap();
    assert!(
        blockers.iter().any(|blocker| blocker == "cache-is-active"),
        "an open cache payload must veto native prune authority: {plan}"
    );
}

#[test]
fn running_tool_with_cache_path_argument_blocks_native_prune_without_open_handle() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    fs::create_dir(&cache).unwrap();
    fs::write(cache.join("tool.bin"), b"cached executable payload").unwrap();
    let canonical_cache = fs::canonicalize(&cache).unwrap();

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

    // Model a uv-managed tool process whose command identity refers to the cache while it keeps no
    // cache file descriptor open. The shared process-command probe must still veto pruning.
    let mut active_tool = Command::new("/bin/sh")
        .args(["-c", "while :; do sleep 1; done", "uv-tool"])
        .arg(canonical_cache.join("archive-v0/tool/bin/tool"))
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));

    let binary = env!("CARGO_BIN_EXE_disksage-uv-cache-reclaim");
    let output = Command::new(binary)
        .args(["--uv-bin"])
        .arg(&uv)
        .output()
        .unwrap();
    let _ = active_tool.kill();
    let _ = active_tool.wait();

    assert!(
        output.status.success(),
        "planning should return active process evidence: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["active_use"]["active"], true);
    assert_eq!(plan["active_use"]["evidence_complete"], true);
    assert!(plan["active_use"]["observed_pids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|pid| pid.as_u64() == Some(active_tool.id() as u64)));
    assert!(plan["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|blocker| blocker == "cache-is-active"));
}

#[test]
fn postcheck_failure_after_prune_still_emits_auditable_receipt() {
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
            "#!/bin/sh\nset -eu\nif [ \"${{1:-}}\" = '--version' ]; then printf 'uv 0.8.0\\n'; exit 0; fi\nif [ \"${{1:-}}\" = 'cache' ] && [ \"${{2:-}}\" = 'dir' ]; then printf '%s\\n' '{}'; exit 0; fi\nif [ \"${{1:-}}\" = 'cache' ] && [ \"${{2:-}}\" = 'prune' ]; then rm -rf '{}'; printf 'pruned\\n'; exit 0; fi\nexit 64\n",
            cache.display(),
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
    assert!(plan_output.status.success());
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
        .arg("preserve mutation evidence when capacity postcheck fails")
        .arg("--record-dir")
        .arg(&records)
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "incomplete postcheck must be fail-closed to automation"
    );
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("a mutation attempt must still emit its receipt");
    assert_eq!(receipt["status_code"], 0);
    assert_eq!(receipt["stdout"], "pruned\n");
    assert!(receipt["filesystem_available_after_bytes"].is_null());
    assert!(receipt["filesystem_available_delta_bytes"].is_null());
    assert_eq!(
        receipt["capacity_postcheck_error"],
        "uv-cache-reclaim-filesystem-capacity-unavailable"
    );
    let result_path = receipt["result_record_path"].as_str().unwrap();
    assert!(PathBuf::from(result_path).is_file());
    assert!(String::from_utf8_lossy(&output.stderr).contains("uv-cache-reclaim-command-failed"));
}
