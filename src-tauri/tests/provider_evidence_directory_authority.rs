#[cfg(unix)]
fn valid_provider_evidence() -> disksage_lib::cloud_transfer::ProviderSyncEvidence {
    use disksage_lib::cloud::CloudProvider;
    use disksage_lib::cloud_transfer::{
        ProviderSyncEvidence, RemoteChecksumAlgorithm, RemoteContentProof, SyncEvidenceKind,
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
fn provider_evidence_symlink_directory_fails_closed_without_publication() {
    use disksage_lib::provider_evidence::write_immutable_sync_evidence;
    use std::os::unix::fs::{symlink, PermissionsExt};

    let fixture = tempfile::tempdir().expect("temporary provider evidence fixture");
    let real_directory = fixture.path().join("real-provider-evidence");
    let linked_directory = fixture.path().join("linked-provider-evidence");
    std::fs::create_dir(&real_directory).expect("create real provider evidence directory");
    std::fs::set_permissions(
        &real_directory,
        std::fs::Permissions::from_mode(0o700),
    )
    .expect("make provider evidence directory private");
    symlink(&real_directory, &linked_directory).expect("create provider evidence symlink");

    let error = write_immutable_sync_evidence(&linked_directory, &valid_provider_evidence())
        .expect_err("symlink provider evidence authority must fail closed");

    assert_eq!(error, "provider-evidence-directory-unsafe");
    assert_eq!(
        std::fs::read_dir(&real_directory)
            .expect("real provider evidence directory remains readable")
            .count(),
        0,
        "symlink refusal must not publish through its target"
    );
}
