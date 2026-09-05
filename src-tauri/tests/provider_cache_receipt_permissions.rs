#![cfg(unix)]

use disksage_lib::provider_cache::{
    execute_trash, plan_with_runtime, ProviderCacheCleanupRequest, ProviderCacheKind,
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
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
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
fn trash_receipt_requires_preprovisioned_private_parent_and_is_not_owner_writable() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let applications = temp.path().join("Applications");
    let active_raw = temp.path().join("active.raw");
    fs::write(&active_raw, b"active").unwrap();
    let podman = fake_podman(temp.path(), &active_raw);

    let seed_bytes = b"recreatable-podman-machine-seed";
    let seed_digest = sha256_hex(seed_bytes);
    let seed_cache = home.join(".local/share/containers/podman/machine/applehv/cache");
    fs::create_dir_all(&seed_cache).unwrap();
    fs::write(seed_cache.join(format!("{seed_digest}.raw.zst")), seed_bytes).unwrap();

    // Active-use evidence is part of the cleanup authority. Keep this regression independent of
    // runner packages by providing deterministic no-holder `lsof` and empty `ps` evidence.
    let fake_bin = temp.path().join("fake-bin");
    fs::create_dir(&fake_bin).unwrap();
    executable(&fake_bin.join("lsof"), "#!/bin/sh\nexit 1\n");
    executable(&fake_bin.join("ps"), "#!/bin/sh\nexit 0\n");
    let _path_guard = PathGuard(std::env::var_os("PATH"));
    unsafe { std::env::set_var("PATH", &fake_bin) };

    let plan = plan_with_runtime(&home, &applications, &podman, 1);
    assert!(plan.evidence_complete, "{:?}", plan.issues);
    assert!(plan.exact_approval_phrase.is_none());
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
    let receipt_dir = data.join("receipts");

    let error = execute_trash(
        &home,
        &applications,
        &podman,
        std::slice::from_ref(&request),
        &plan.plan_fingerprint,
        &plan.plan_fingerprint,
        plan.trash_approval_phrase.as_deref().unwrap(),
        "verified regenerable provider cache",
        &data.join("journal.jsonl"),
        &receipt_dir,
        2,
    )
    .unwrap_err();
    assert_eq!(
        error,
        "provider-cache-receipt-object-bound-publication-failed"
    );
    assert!(
        !receipt_dir.exists(),
        "provider-cache must not create receipt parents through pathname authority"
    );
    assert!(
        Path::new(&request.path).exists(),
        "receipt admission failure must happen before cache mutation"
    );

    fs::create_dir(&receipt_dir).unwrap();
    fs::set_permissions(&receipt_dir, fs::Permissions::from_mode(0o700)).unwrap();

    let result = execute_trash(
        &home,
        &applications,
        &podman,
        std::slice::from_ref(&request),
        &plan.plan_fingerprint,
        &plan.plan_fingerprint,
        plan.trash_approval_phrase.as_deref().unwrap(),
        "verified regenerable provider cache",
        &data.join("journal.jsonl"),
        &receipt_dir,
        3,
    )
    .unwrap();

    let mode = fs::metadata(&result.immutable_receipt_path)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o222, 0, "immutable receipt retained write bits");
}
