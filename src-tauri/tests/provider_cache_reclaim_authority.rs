#![cfg(unix)]

use disksage_lib::provider_cache::{
    execute_trash, plan_with_runtime, ProviderCacheCleanupRequest, ProviderCacheKind,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn fake_podman(temp: &Path, active_image: &Path) -> PathBuf {
    let config = temp.join("podman-config");
    fs::create_dir_all(&config).expect("create Podman config fixture");
    fs::write(
        config.join("podman-machine-default.json"),
        format!(r#"{{"ImagePath":{{"Path":"{}"}}}}"#, active_image.display()),
    )
    .expect("write Podman machine config");
    let podman = temp.join("podman");
    fs::write(
        &podman,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'podman version test'; else printf '[{{\"Name\":\"podman-machine-default\",\"ConfigDir\":{{\"Path\":\"{}\"}}}}]'; fi\n",
            config.display()
        ),
    )
    .expect("write fake Podman");
    fs::set_permissions(&podman, fs::Permissions::from_mode(0o700))
        .expect("make fake Podman executable");
    podman
}

#[test]
fn podman_seed_filename_must_match_the_file_digest() {
    let temp = tempfile::tempdir().expect("temporary provider-cache fixture");
    let home = temp.path().join("home");
    let applications = temp.path().join("Applications");
    let active_image = temp.path().join("active.raw");
    fs::write(&active_image, b"active-image").expect("write active image");
    let podman = fake_podman(temp.path(), &active_image);
    let cache = home.join(".local/share/containers/podman/machine/applehv/cache");
    fs::create_dir_all(&cache).expect("create Podman seed cache");
    let name = format!("{}.raw.zst", "0".repeat(64));
    fs::write(cache.join(&name), b"not-the-zero-digest").expect("write corrupt seed fixture");

    let plan = plan_with_runtime(&home, &applications, &podman, 1);

    assert!(plan
        .issues
        .iter()
        .any(|issue| issue == &format!("podman-seed-cache-digest-mismatch:{name}")));
    assert!(!plan.evidence_complete);
    assert!(plan
        .candidates
        .iter()
        .all(|candidate| candidate.kind != ProviderCacheKind::PodmanMachineSeed));
}

#[test]
fn symlinked_podman_cache_root_is_rejected_before_enumeration() {
    let temp = tempfile::tempdir().expect("temporary provider-cache fixture");
    let home = temp.path().join("home");
    let applications = temp.path().join("Applications");
    let active_image = temp.path().join("active.raw");
    fs::write(&active_image, b"active-image").expect("write active image");
    let podman = fake_podman(temp.path(), &active_image);
    let external = temp.path().join("external-cache");
    fs::create_dir_all(&external).expect("create external cache fixture");
    let seed_bytes = b"regenerable-seed";
    let digest = sha256_hex(seed_bytes);
    fs::write(external.join(format!("{digest}.raw.zst")), seed_bytes)
        .expect("write external seed fixture");
    let cache = home.join(".local/share/containers/podman/machine/applehv/cache");
    fs::create_dir_all(cache.parent().expect("cache parent")).expect("create cache parent");
    symlink(&external, &cache).expect("symlink provider cache root");

    let plan = plan_with_runtime(&home, &applications, &podman, 1);

    assert!(plan
        .issues
        .iter()
        .any(|issue| issue == "podman-seed-cache-root-symlink-rejected"));
    assert!(!plan.evidence_complete);
    assert!(plan
        .candidates
        .iter()
        .all(|candidate| candidate.kind != ProviderCacheKind::PodmanMachineSeed));
}

#[test]
fn configured_podman_image_is_never_a_seed_cleanup_candidate() {
    let temp = tempfile::tempdir().expect("temporary provider-cache fixture");
    let home = temp.path().join("home");
    let applications = temp.path().join("Applications");
    let cache = home.join(".local/share/containers/podman/machine/applehv/cache");
    fs::create_dir_all(&cache).expect("create Podman seed cache");
    let image_bytes = b"configured-machine-image";
    let digest = sha256_hex(image_bytes);
    let active_image = cache.join(format!("{digest}.raw.zst"));
    fs::write(&active_image, image_bytes).expect("write configured image fixture");
    let podman = fake_podman(temp.path(), &active_image);

    let plan = plan_with_runtime(&home, &applications, &podman, 1);

    assert!(plan
        .issues
        .iter()
        .any(|issue| issue == "podman-seed-cache-configured-image-excluded"));
    assert!(!plan.evidence_complete);
    assert!(plan
        .candidates
        .iter()
        .all(|candidate| candidate.path != active_image.to_string_lossy()));
}

#[test]
fn cleanup_request_manifest_is_bounded_before_replanning() {
    let temp = tempfile::tempdir().expect("temporary provider-cache fixture");
    let requests = (0..=1024)
        .map(|index| ProviderCacheCleanupRequest {
            path: format!("/tmp/provider-cache-{index}"),
            evidence_fingerprint: format!("fingerprint-{index}"),
            object_id: format!("object-{index}"),
        })
        .collect::<Vec<_>>();

    let error = execute_trash(
        &temp.path().join("home"),
        &temp.path().join("Applications"),
        Path::new("/missing/podman"),
        &requests,
        "untrusted-plan",
        "untrusted-plan",
        "untrusted-confirmation",
        "bounded request regression",
        &temp.path().join("journal.jsonl"),
        &temp.path().join("receipts"),
        1,
    )
    .expect_err("oversized manifest must fail closed before evidence collection");

    assert_eq!(error, "provider-cache-cleanup-request-count-exceeds-limit");
}
