#[cfg(unix)]
fn valid_approval() -> disksage_lib::cloud_eviction::CloudSourceEvictionApproval {
    use disksage_lib::cloud_eviction::CloudSourceEvictionApproval;
    use disksage_lib::cloud_local_eviction::ActiveUseEvidence;

    let mut approval = CloudSourceEvictionApproval {
        version: 1,
        approval_id: String::new(),
        receipt_id: "a".repeat(64),
        evidence_record_id: "b".repeat(64),
        approved_at_ms: 20,
        approved_by: "human:local:test".into(),
        rationale: "reviewed exact source eviction authority".into(),
        active_use_observed_at_ms: 19,
        active_use: ActiveUseEvidence {
            method: "lsof-fp+ps-command".into(),
            evidence_complete: true,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: None,
        },
    };

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-cloud-source-eviction-approval-v1\0");
    for value in [
        approval.receipt_id.as_str(),
        approval.evidence_record_id.as_str(),
        approval.approved_by.as_str(),
        approval.rationale.as_str(),
        approval.active_use.method.as_str(),
        approval.active_use.error.as_deref().unwrap_or_default(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    hasher.update(&approval.approved_at_ms.to_le_bytes());
    hasher.update(&approval.active_use_observed_at_ms.to_le_bytes());
    hasher.update(&[
        approval.active_use.evidence_complete as u8,
        approval.active_use.active as u8,
        approval.active_use.results_truncated as u8,
        approval.active_use.error.is_some() as u8,
    ]);
    approval.approval_id = hasher.finalize().to_hex().to_string();
    approval
}

#[cfg(unix)]
#[test]
fn shared_writable_eviction_record_directory_fails_closed() {
    use disksage_lib::cloud_eviction::write_immutable_source_eviction_approval;
    use std::os::unix::fs::PermissionsExt;

    for unsafe_write_bit in [0o020, 0o002] {
        let directory = tempfile::tempdir().expect("temporary eviction authority directory");
        let mut permissions = std::fs::metadata(directory.path())
            .expect("eviction authority directory metadata")
            .permissions();
        permissions.set_mode(0o700 | unsafe_write_bit);
        std::fs::set_permissions(directory.path(), permissions)
            .expect("make eviction authority directory shared-writable for regression");

        let error = write_immutable_source_eviction_approval(directory.path(), &valid_approval())
            .expect_err("shared-writable eviction authority directory must fail closed");

        assert_eq!(error, "eviction-dir-writable-by-others");
        assert_eq!(
            std::fs::read_dir(directory.path())
                .expect("eviction authority directory remains readable")
                .count(),
            0,
            "refusing unsafe authority storage must not create an approval record"
        );
    }
}

#[cfg(unix)]
#[test]
fn successful_eviction_approval_is_owner_read_only_and_create_once_at_runtime() {
    use disksage_lib::cloud_eviction::write_immutable_source_eviction_approval;
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().expect("temporary eviction authority directory");
    let approval = valid_approval();

    write_immutable_source_eviction_approval(directory.path(), &approval)
        .expect("valid approval must be written once");

    let entries = std::fs::read_dir(directory.path())
        .expect("eviction authority directory remains readable")
        .collect::<Result<Vec<_>, _>>()
        .expect("eviction authority entries remain readable");
    assert_eq!(entries.len(), 1);
    let path = entries[0].path();
    let metadata = std::fs::symlink_metadata(&path).expect("approval metadata");
    assert!(metadata.is_file());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o400);
    assert!(metadata.permissions().readonly());

    let stored: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&path).expect("approval file remains readable by owner"),
    )
    .expect("approval record remains valid JSON");
    assert_eq!(stored["approval_id"], approval.approval_id);
    assert_eq!(stored["receipt_id"], approval.receipt_id);

    write_immutable_source_eviction_approval(directory.path(), &approval)
        .expect_err("immutable approval must not be overwritten");
}

#[cfg(unix)]
#[test]
fn eviction_authority_file_is_private_from_creation_and_object_bound_for_hardening() {
    let source = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/cloud_eviction.rs"),
    )
    .expect("cloud eviction source must be readable");

    assert!(
        source.contains("options.mode(0o400);"),
        "eviction authority records must be owner-read-only from create_new rather than only after a later chmod"
    );
    assert!(
        source.contains("file.set_permissions(permissions)"),
        "post-write hardening must stay bound to the opened authority file"
    );
    assert!(
        !source.contains("std::fs::set_permissions(path, permissions)"),
        "authority hardening must not re-resolve a replaceable pathname after create_new"
    );
}
