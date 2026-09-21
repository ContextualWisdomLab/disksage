#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

fn write_fake_uv(path: &Path, cache: &Path, tools: &Path, prune_log: &Path) {
    fs::write(
        path,
        format!(
            "#!/bin/sh\nset -eu\nif [ \"${{1:-}}\" = '--version' ]; then printf 'uv 0.test\\n'; exit 0; fi\nif [ \"${{1:-}} ${{2:-}}\" = 'cache dir' ]; then printf '%s\\n' '{}'; exit 0; fi\nif [ \"${{1:-}} ${{2:-}}\" = 'tool dir' ]; then printf '%s\\n' '{}'; exit 0; fi\nif [ \"${{1:-}} ${{2:-}}\" = 'cache prune' ]; then printf '%s\\n' \"$*\" >> '{}'; printf 'pruned 0 files\\n'; exit 0; fi\nexit 64\n",
            cache.display(),
            tools.display(),
            prune_log.display()
        ),
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    let tools = temp.path().join("tools");
    let home = temp.path().join("home");
    let records = temp.path().join("records");
    let prune_log = temp.path().join("prune.log");
    fs::create_dir(&cache).unwrap();
    fs::create_dir(&tools).unwrap();
    fs::create_dir(&home).unwrap();
    fs::create_dir(&records).unwrap();
    fs::set_permissions(&records, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(cache.join("payload.whl"), b"cached").unwrap();
    let uv = temp.path().join("uv");
    write_fake_uv(&uv, &cache, &tools, &prune_log);
    (temp, cache, tools, home, records, uv)
}

fn plan(binary: &str, uv: &Path, home: &Path) -> serde_json::Value {
    let output = Command::new(binary)
        .arg("--uv-bin")
        .arg(uv)
        .env("HOME", home)
        .env_remove("UV_LINK_MODE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "plan failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn open_cached_payload_blocks_native_prune() {
    let (_temp, cache, _tools, home, _records, uv) = fixture();
    let payload = cache.join("payload.whl");
    let _open_payload = fs::File::open(payload).unwrap();

    let value = plan(env!("CARGO_BIN_EXE_disksage-uv-cache-reclaim"), &uv, &home);

    assert_eq!(value["active_use"]["active"], true);
    assert!(value["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|blocker| blocker == "cache-is-active"));
}

#[test]
fn persistent_service_reference_blocks_native_prune() {
    let (_temp, cache, _tools, home, _records, uv) = fixture();
    let tool = cache.join("archive-v0").join("mcp").join("bin").join("server");
    fs::create_dir_all(tool.parent().unwrap()).unwrap();
    fs::write(&tool, b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();

    #[cfg(target_os = "linux")]
    {
        let units = home.join(".config").join("systemd").join("user");
        fs::create_dir_all(&units).unwrap();
        fs::write(
            units.join("mcp.service"),
            format!("[Service]\nExecStart={} --serve\n", tool.display()),
        )
        .unwrap();
    }
    #[cfg(target_os = "macos")]
    {
        let agents = home.join("Library").join("LaunchAgents");
        fs::create_dir_all(&agents).unwrap();
        fs::write(
            agents.join("com.example.mcp.plist"),
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>com.example.mcp</string><key>Program</key><string>{}</string></dict></plist>\n",
                tool.display()
            ),
        )
        .unwrap();
    }

    let value = plan(env!("CARGO_BIN_EXE_disksage-uv-cache-reclaim"), &uv, &home);

    assert_eq!(value["persistent_service_evidence_complete"], true);
    assert_eq!(value["persistent_service_cache_dependency_count"], 1);
    assert!(value["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|blocker| blocker == "persistent-service-cache-dependency"));
}

#[test]
fn persistent_tool_symlink_into_cache_blocks_native_prune() {
    let (_temp, cache, tools, home, _records, uv) = fixture();
    let cached = cache.join("archive-v0").join("package.py");
    fs::create_dir_all(cached.parent().unwrap()).unwrap();
    fs::write(&cached, b"cached").unwrap();
    let installed = tools.join("mcp").join("lib");
    fs::create_dir_all(&installed).unwrap();
    symlink(&cached, installed.join("package.py")).unwrap();

    let value = plan(env!("CARGO_BIN_EXE_disksage-uv-cache-reclaim"), &uv, &home);

    assert_eq!(value["persistent_tool_cache_symlink_count"], 1);
    assert!(value["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|blocker| blocker == "persistent-tool-cache-symlink-coupling"));
}

#[test]
fn native_prune_invokes_uv_not_private_bucket_deletion() {
    let (temp, _cache, _tools, home, records, uv) = fixture();
    let binary = env!("CARGO_BIN_EXE_disksage-uv-cache-reclaim");
    let value = plan(binary, &uv, &home);
    assert_eq!(value["blockers"].as_array().unwrap().len(), 0, "{value}");

    let output = Command::new(binary)
        .arg("--uv-bin")
        .arg(&uv)
        .arg("--execute")
        .arg("--approved-plan-fingerprint")
        .arg(value["plan_fingerprint"].as_str().unwrap())
        .arg("--confirm")
        .arg(value["exact_approval_phrase"].as_str().unwrap())
        .arg("--approved-by")
        .arg("human:test")
        .arg("--rationale")
        .arg("prove native uv owner controls cache mutation")
        .arg("--record-dir")
        .arg(&records)
        .env("HOME", &home)
        .env_remove("UV_LINK_MODE")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "execute failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["status_code"], 0);
    let prune_log = temp.path().join("prune.log");
    let invoked = fs::read_to_string(prune_log).unwrap();
    assert!(invoked.contains("cache prune"), "{invoked}");
    assert!(invoked.contains("--cache-dir"), "{invoked}");
    assert!(!invoked.contains("--force"), "{invoked}");
    assert!(records.read_dir().unwrap().count() >= 2);
}
