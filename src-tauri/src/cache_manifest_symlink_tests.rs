use std::fs;

#[test]
fn nested_symlink_is_manifested_without_hiding_the_cache_target() {
    let temp = tempfile::tempdir().expect("temporary cache-manifest fixture");
    let cache_root = temp.path().join("cache-root");
    let target = cache_root.join("toolchain-jdk");
    let outside = temp.path().join("outside-user-data");
    fs::create_dir_all(target.join("bin")).expect("create generated cache target");
    fs::write(target.join("bin").join("java"), b"generated-runtime")
        .expect("write generated runtime fixture");
    fs::create_dir(&outside).expect("create outside fixture");
    fs::write(outside.join("keep.txt"), b"must-not-be-followed")
        .expect("write outside fixture");
    std::os::unix::fs::symlink(&outside, target.join("external-link"))
        .expect("create nested symlink fixture");

    let targets = crate::rules::cache_targets(&cache_root).expect("enumerate cache targets");

    assert_eq!(
        targets.len(),
        1,
        "a nested symlink must be represented without hiding the enclosing generated cache target"
    );
    assert_eq!(targets[0].path, target.to_string_lossy());
    assert_eq!(
        fs::read(outside.join("keep.txt")).expect("outside fixture must remain readable"),
        b"must-not-be-followed",
        "manifest construction must not follow or mutate a nested symlink target"
    );
}

#[test]
fn manifest_variable_fields_are_length_framed() {
    use std::os::unix::ffi::OsStrExt;

    fn update_framed(hasher: &mut blake3::Hasher, bytes: &[u8]) {
        let length = u64::try_from(bytes.len()).expect("fixture field length fits u64");
        hasher.update(&length.to_le_bytes());
        hasher.update(bytes);
    }

    let temp = tempfile::tempdir().expect("temporary cache-manifest fixture");
    let cache_root = temp.path().join("cache-root");
    fs::create_dir(&cache_root).expect("create cache root");
    let target_path = cache_root.join("ab");
    fs::write(&target_path, b"generated-payload").expect("write generated cache fixture");

    let catalog_targets =
        crate::rules_catalog::cache_targets(&cache_root).expect("enumerate catalog cache targets");
    assert_eq!(catalog_targets.len(), 1);
    let catalog_target = &catalog_targets[0];

    let mut expected = blake3::Hasher::new();
    update_framed(
        &mut expected,
        target_path
            .file_name()
            .expect("fixture has a file name")
            .as_bytes(),
    );
    expected.update(&[0]);
    let metadata = fs::symlink_metadata(&target_path).expect("read generated cache metadata");
    update_framed(
        &mut expected,
        crate::rules_catalog::cache_metadata_fingerprint(&metadata).as_bytes(),
    );
    update_framed(&mut expected, catalog_target.object_id.as_bytes());

    assert_eq!(
        catalog_target.manifest_fingerprint,
        expected.finalize().to_hex().to_string(),
        "catalog cache manifest must length-frame variable fields before hashing"
    );

    let authority_targets =
        crate::rules::cache_targets(&cache_root).expect("upgrade cache target authority");
    assert_eq!(authority_targets.len(), 1);
    assert!(
        crate::rules::cache_manifest_components(&authority_targets[0].manifest_fingerprint)
            .is_some(),
        "the production cache boundary must wrap the framed catalog manifest in versioned authority"
    );
}

#[test]
fn directory_authority_binds_catalog_manifest_fingerprint() {
    fn update_framed(hasher: &mut blake3::Hasher, bytes: &[u8]) {
        let length = u64::try_from(bytes.len()).expect("fixture field length fits u64");
        hasher.update(&length.to_le_bytes());
        hasher.update(bytes);
    }

    let temp = tempfile::tempdir().expect("temporary directory-authority fixture");
    let cache_root = temp.path().join("cache-root");
    let target_path = cache_root.join("generated-cache");
    fs::create_dir_all(&target_path).expect("create generated cache directory");
    fs::write(target_path.join("payload.bin"), b"generated-payload")
        .expect("write generated cache payload");

    let catalog_targets =
        crate::rules_catalog::cache_targets(&cache_root).expect("enumerate catalog cache targets");
    let authority_targets =
        crate::rules::cache_targets(&cache_root).expect("upgrade cache target authority");
    assert_eq!(catalog_targets.len(), 1);
    assert_eq!(authority_targets.len(), 1);
    assert_eq!(catalog_targets[0].path, authority_targets[0].path);

    let (_, stable_tree) = crate::rules::cache_manifest_components(
        &authority_targets[0].manifest_fingerprint,
    )
    .expect("parse versioned directory authority");
    let metadata = fs::symlink_metadata(&target_path).expect("read generated cache metadata");
    let mut expected = blake3::Hasher::new();
    expected.update(b"disksage-cache-directory-authority-v2\0");
    update_framed(
        &mut expected,
        catalog_targets[0].manifest_fingerprint.as_bytes(),
    );
    update_framed(
        &mut expected,
        crate::rules::cache_root_relocation_metadata_fingerprint(&metadata).as_bytes(),
    );

    assert_eq!(
        stable_tree,
        expected.finalize().to_hex().as_str(),
        "directory destructive authority must cryptographically bind the reviewed catalog manifest"
    );
}

#[test]
fn reviewed_directory_snapshot_binds_root_ctime_before_staging() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let temp = tempfile::tempdir().expect("temporary root-metadata fixture");
    let target = temp.path().join("generated-cache");
    fs::create_dir(&target).expect("create generated cache target");
    fs::write(target.join("payload.bin"), b"generated")
        .expect("write generated cache payload");

    let reviewed = crate::rules::cache_target(&target).expect("snapshot reviewed cache target");
    let before = fs::symlink_metadata(&target).expect("read reviewed root metadata");
    let original_mode = before.permissions().mode() & 0o7777;
    let temporary_mode = if original_mode & 0o100 != 0 {
        original_mode & !0o100
    } else {
        original_mode | 0o100
    };
    let mut permissions = before.permissions();
    permissions.set_mode(temporary_mode);
    fs::set_permissions(&target, permissions).expect("temporarily mutate root metadata");
    let mut permissions = fs::symlink_metadata(&target)
        .expect("read temporary root metadata")
        .permissions();
    permissions.set_mode(original_mode);
    fs::set_permissions(&target, permissions).expect("restore reviewed permission bits");

    let after = fs::symlink_metadata(&target).expect("read changed root metadata");
    assert_ne!(
        (before.ctime(), before.ctime_nsec()),
        (after.ctime(), after.ctime_nsec()),
        "fixture must produce a ctime-only root metadata transition"
    );
    let live = crate::rules::cache_target(&target).expect("snapshot live cache target");
    assert_eq!(reviewed.object_id, live.object_id);
    assert_eq!(reviewed.modified_ms, live.modified_ms);
    assert_ne!(
        reviewed, live,
        "reviewed root metadata changes must invalidate destructive authority before staging"
    );
}

#[test]
fn permanent_delete_rejects_ctime_only_root_drift() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let temp = tempfile::tempdir().expect("temporary permanent-delete fixture");
    let target_path = temp.path().join("generated-cache");
    fs::create_dir(&target_path).expect("create generated cache target");
    fs::write(target_path.join("payload.bin"), b"generated")
        .expect("write generated cache payload");
    let reviewed = crate::rules::cache_target(&target_path).expect("snapshot reviewed cache target");
    let before = fs::symlink_metadata(&target_path).expect("read reviewed root metadata");
    let original_mode = before.permissions().mode() & 0o7777;
    let temporary_mode = if original_mode & 0o100 != 0 {
        original_mode & !0o100
    } else {
        original_mode | 0o100
    };
    let mut permissions = before.permissions();
    permissions.set_mode(temporary_mode);
    fs::set_permissions(&target_path, permissions).expect("temporarily mutate root metadata");
    let mut permissions = fs::symlink_metadata(&target_path)
        .expect("read temporary root metadata")
        .permissions();
    permissions.set_mode(original_mode);
    fs::set_permissions(&target_path, permissions).expect("restore reviewed permission bits");
    let after = fs::symlink_metadata(&target_path).expect("read changed root metadata");
    assert_ne!(
        (before.ctime(), before.ctime_nsec()),
        (after.ctime(), after.ctime_nsec()),
        "fixture must produce a ctime-only root metadata transition"
    );

    let journal = temp.path().join("journal.jsonl");
    let result = crate::safety::permanent_delete_dir_if_identity(
        &target_path,
        &reviewed.object_id,
        reviewed.bytes,
        reviewed.modified_ms,
        &reviewed.manifest_fingerprint,
        &journal,
        77,
    );

    assert!(
        result.is_err(),
        "stale reviewed root metadata must not authorize irreversible deletion"
    );
    assert!(target_path.exists(), "the stale target must remain intact");
}
