#[cfg(unix)]
#[test]
fn shared_writable_provider_evidence_directory_fails_closed() {
    use disksage_lib::cloud::CloudProvider;
    use disksage_lib::cloud_transfer::{
        ProviderSyncEvidence, RemoteChecksumAlgorithm, RemoteContentProof, SyncEvidenceKind,
    };
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

        let evidence = ProviderSyncEvidence {
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
        };

        let error = write_immutable_sync_evidence(directory.path(), &evidence)
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
