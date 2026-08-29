#![cfg(all(unix, not(coverage)))]

use disksage_lib::provider_cache_reclaim::{
    execute, plan_with_runtime, ProviderCacheCleanupMode, ProviderCacheCleanupRequest,
    ProviderCacheKind,
};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

struct PathGuard(Option<OsString>);

impl Drop for PathGuard {
    fn drop(&mut self) {
        match self.0.take() {
            Some(path) => unsafe { std::env::set_var("PATH", path) },
            None => unsafe { std::env::remove_var("PATH") },
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn fake_podman(temp: &Path, active_raw: &Path) -> PathBuf {
    let config = temp.join("podman-config");
    fs::create_dir_all(&config).unwrap();
    fs::write(
        config.join("podman-machine-default.json"),
        format!(r#"{{"ImagePath":{{"Path":"{}"}}}}"#, active_raw.display()),
    )
    .unwrap();
    let podman = temp.join("podman");
    executable(
        &podman,
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'podman version test'; else printf '[{{\"Name\":\"podman-machine-default\",\"ConfigDir\":{{\"Path\":\"{}\"}}}}]'; fi\n",
            config.display()
        ),
    );
    podman
}

#[test]
fn permanent_purge_preserves_same_inode_seed_changed_after_replan() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let applications = temp.path().join("Applications");
    let active_raw = temp.path().join("active.raw");
    fs::write(&active_raw, b"active").unwrap();
    let podman = fake_podman(temp.path(), &active_raw);

    let original = b"recreatable-podman-machine-seed";
    let changed = b"changed-after-replan-same-inode";
    let digest = sha256_hex(original);
    let seed_cache = home.join(".local/share/containers/podman/machine/applehv/cache");
    fs::create_dir_all(&seed_cache).unwrap();
    let seed = seed_cache.join(format!("{digest}.raw.zst"));
    fs::write(&seed, original).unwrap();

    let fake_bin = temp.path().join("fake-bin");
    fs::create_dir(&fake_bin).unwrap();
    let counter = temp.path().join("lsof-count");
    executable(
        &fake_bin.join("lsof"),
        &format!(
            "#!/bin/sh\ncount=$(cat '{}' 2>/dev/null || echo 0)\ncount=$((count + 1))\nprintf '%s' \"$count\" > '{}'\nif [ \"$count\" -eq 3 ]; then printf '%s' 'changed-after-replan-same-inode' > '{}'; fi\nexit 1\n",
            counter.display(),
            counter.display(),
            seed.display()
        ),
    );
    executable(&fake_bin.join("ps"), "#!/bin/sh\nexit 0\n");
    let _path_guard = PathGuard(std::env::var_os("PATH"));
    unsafe { std::env::set_var("PATH", &fake_bin) };

    let plan = plan_with_runtime(&home, &applications, &podman, 1);
    assert!(plan.evidence_complete, "{:?}", plan.issues);
    let candidate = plan
        .candidates
        .iter()
        .find(|candidate| candidate.kind == ProviderCacheKind::PodmanMachineSeed)
        .unwrap();
    let request = ProviderCacheCleanupRequest {
        path: candidate.path.clone(),
        evidence_fingerprint: candidate.evidence_fingerprint.clone(),
        object_id: candidate.object_id.clone(),
    };
    let data = temp.path().join("data");
    fs::create_dir_all(&data).unwrap();

    let result = execute(
        &home,
        &applications,
        &podman,
        &[request],
        &plan.plan_fingerprint,
        &plan.plan_fingerprint,
        plan.exact_approval_phrase.as_deref().unwrap(),
        "verified regenerable provider cache",
        &data.join("journal.jsonl"),
        &data.join("receipts"),
        ProviderCacheCleanupMode::PermanentPurge,
        2,
    )
    .unwrap();

    assert_eq!(result.completed_count, 0);
    assert_eq!(
        result.items[0].error.as_deref(),
        Some("provider-cache-staged-content-changed")
    );
    assert_eq!(fs::read(&seed).unwrap(), changed);
}
