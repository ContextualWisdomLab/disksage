#[cfg(unix)]
fn valid_provider_evidence() -> disksage_lib::cloud_transfer::ProviderSyncEvidence {
    use disksage_lib::cloud::CloudProvider;
    use disksage_lib::cloud_transfer::{
        ProviderSyncEvidence, ProviderSyncState, RemoteChecksumAlgorithm, RemoteContentProof,
        SyncEvidenceKind,
    };

    ProviderSyncEvidence {
        receipt_id: "a".repeat(64),
        provider: CloudProvider::Onedrive,
        destination: "/cloud/report.pdf".into(),
        observed_bytes: 42,
        destination_blake3: "b".repeat(64),
        confirmed_at_ms: 30,
        kind: SyncEvidenceKind::ProviderApi,
        evidence_id: format!("provider-api:{}", "c".repeat(64)),
        sync_complete: true,
        sync_state: ProviderSyncState::Complete,
        remote_content: Some(RemoteContentProof {
            object_id: "remote-id".into(),
            revision: "revision-1".into(),
            algorithm: RemoteChecksumAlgorithm::QuickXor,
            checksum: "quick-xor".into(),
            location_bound: true,
            location_proof: Some(format!("onedrive-path-v1:{}", "d".repeat(64))),
        }),
    }
}

#[cfg(unix)]
#[test]
fn shared_writable_provider_evidence_directory_fails_closed() {
    use disksage_lib::provider_evidence::write_immutable_sync_evidence;
    use std::os::unix::fs::PermissionsExt;

    for unsafe_write_bit in [0o020, 0o002] {
        let directory = tempfile::tempdir().expect("temporary provider evidence directory");
        let mut permissions = std::fs::metadata(directory.path())
            .expect("provider evidence directory metadata")
            .permissions();
        permissions.set_mode(0o700 | unsafe_write_bit);
        std::fs::set_permissions(directory.path(), permissions)
            .expect("make provider evidence directory shared-writable for regression");

        let error = write_immutable_sync_evidence(directory.path(), &valid_provider_evidence())
            .expect_err("shared-writable provider evidence authority must fail closed");

        assert_eq!(error, "provider-evidence-directory-writable-by-others");
        assert_eq!(
            std::fs::read_dir(directory.path())
                .expect("provider evidence directory remains readable")
                .count(),
            0,
            "refusing the unsafe directory must not create an evidence file"
        );
    }
}

#[cfg(unix)]
#[test]
fn latest_api_object_id_rejects_directory_that_became_shared_writable() {
    use disksage_lib::cloud::CloudProvider;
    use disksage_lib::provider_evidence::{latest_api_object_id, write_immutable_sync_evidence};
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().expect("temporary provider evidence directory");
    let evidence = valid_provider_evidence();
    let (record, _) = write_immutable_sync_evidence(directory.path(), &evidence)
        .expect("valid provider evidence must be written before authority drift");

    let mut permissions = std::fs::metadata(directory.path())
        .expect("provider evidence directory metadata")
        .permissions();
    permissions.set_mode(0o720);
    std::fs::set_permissions(directory.path(), permissions)
        .expect("make provider evidence directory group-writable after publication");

    assert_eq!(
        latest_api_object_id(
            directory.path(),
            &record.evidence.receipt_id,
            CloudProvider::Onedrive,
        ),
        None,
        "locator recovery must not trust evidence after directory authority becomes shared-writable"
    );
}

#[cfg(unix)]
#[test]
fn provider_evidence_is_owner_read_only_and_create_once_at_runtime() {
    use disksage_lib::provider_evidence::{
        read_immutable_sync_evidence, write_immutable_sync_evidence,
    };
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().expect("temporary provider evidence directory");
    let evidence = valid_provider_evidence();

    let (record, path) = write_immutable_sync_evidence(directory.path(), &evidence)
        .expect("valid provider evidence must be written once");
    let metadata = std::fs::symlink_metadata(&path).expect("provider evidence metadata");
    assert!(metadata.is_file());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o400);
    assert!(metadata.permissions().readonly());
    assert_eq!(
        read_immutable_sync_evidence(&path).expect("written evidence must read back"),
        record
    );

    assert_eq!(
        write_immutable_sync_evidence(directory.path(), &evidence).unwrap_err(),
        "provider-evidence-record-create-failed"
    );
}

#[cfg(unix)]
#[test]
fn provider_evidence_file_is_private_from_creation_not_only_after_path_chmod() {
    let source = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/provider_evidence.rs"),
    )
    .expect("provider evidence source must be readable");

    assert!(
        source.contains("options.mode(0o400);"),
        "provider evidence must be created with read-only owner mode atomically, so a crash before post-write chmod cannot leave a broader evidence file"
    );
    assert!(
        source.contains("file.set_permissions(permissions)"),
        "post-write hardening must remain bound to the opened evidence object rather than re-resolving its pathname"
    );
    assert!(
        !source.contains("std::fs::set_permissions(&path, permissions)"),
        "provider evidence hardening must not chmod a pathname that can be replaced after create_new"
    );
}
