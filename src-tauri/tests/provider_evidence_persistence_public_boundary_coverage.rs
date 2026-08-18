//! Public filesystem-boundary coverage for immutable provider synchronization evidence.
//!
//! These tests exercise the production persistence API with real temporary filesystem
//! objects so create-only, bounded, read-only, identity-bound evidence stays fail closed.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::cloud_transfer::{
    ProviderSyncEvidence, RemoteChecksumAlgorithm, RemoteContentProof, SyncEvidenceKind,
};
use disksage_lib::provider_evidence::{
    read_immutable_sync_evidence, write_immutable_sync_evidence,
};
use std::path::Path;

const MAX_PROVIDER_EVIDENCE_RECORD_BYTES: usize = 64 * 1024;

fn absolute_destination() -> String {
    #[cfg(windows)]
    {
        r"C:\cloud\report.pdf".to_string()
    }
    #[cfg(not(windows))]
    {
        "/cloud/report.pdf".to_string()
    }
}

fn provider_api_evidence() -> ProviderSyncEvidence {
    ProviderSyncEvidence {
        receipt_id: "a".repeat(64),
        provider: CloudProvider::Onedrive,
        destination: absolute_destination(),
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

fn make_read_only(path: &Path) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(path, permissions).unwrap();
}

fn make_writable(path: &Path) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(false);
    std::fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn immutable_provider_evidence_round_trip_collision_and_rename_are_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("provider-evidence");
    let evidence = provider_api_evidence();

    let (record, path) = write_immutable_sync_evidence(&directory, &evidence).unwrap();
    assert_eq!(read_immutable_sync_evidence(&path).unwrap(), record);
    assert_eq!(
        write_immutable_sync_evidence(&directory, &evidence).unwrap_err(),
        "provider-evidence-record-create-failed"
    );

    let renamed = directory.join("renamed-evidence.json");
    std::fs::rename(&path, &renamed).unwrap();
    assert_eq!(
        read_immutable_sync_evidence(&renamed).unwrap_err(),
        "provider-evidence-record-filename-id-mismatch"
    );
}

#[test]
fn immutable_provider_evidence_read_rejects_missing_writable_oversized_and_malformed_files() {
    let temp = tempfile::tempdir().unwrap();

    let missing = temp.path().join("missing.json");
    assert_eq!(
        read_immutable_sync_evidence(&missing).unwrap_err(),
        "provider-evidence-record-metadata-failed"
    );

    let writable = temp.path().join("writable.json");
    std::fs::write(&writable, b"{}").unwrap();
    assert_eq!(
        read_immutable_sync_evidence(&writable).unwrap_err(),
        "provider-evidence-record-must-be-read-only-regular-file"
    );

    let directory = temp.path().join("directory.json");
    std::fs::create_dir(&directory).unwrap();
    assert_eq!(
        read_immutable_sync_evidence(&directory).unwrap_err(),
        "provider-evidence-record-must-be-read-only-regular-file"
    );

    let oversized = temp.path().join("oversized.json");
    std::fs::write(&oversized, vec![b'x'; MAX_PROVIDER_EVIDENCE_RECORD_BYTES + 1]).unwrap();
    make_read_only(&oversized);
    assert_eq!(
        read_immutable_sync_evidence(&oversized).unwrap_err(),
        "provider-evidence-record-too-large"
    );

    let malformed = temp.path().join("malformed.json");
    std::fs::write(&malformed, b"{").unwrap();
    make_read_only(&malformed);
    assert_eq!(
        read_immutable_sync_evidence(&malformed).unwrap_err(),
        "provider-evidence-record-json-invalid"
    );
}

#[test]
fn immutable_provider_evidence_write_rejects_record_over_the_durable_size_bound() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("provider-evidence");
    let mut evidence = provider_api_evidence();
    evidence.remote_content.as_mut().unwrap().object_id =
        "x".repeat(MAX_PROVIDER_EVIDENCE_RECORD_BYTES);

    assert_eq!(
        write_immutable_sync_evidence(&directory, &evidence).unwrap_err(),
        "provider-evidence-record-too-large"
    );
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
}

#[test]
fn immutable_provider_evidence_read_revalidates_version_fields_and_digest_after_disk_tamper() {
    let temp = tempfile::tempdir().unwrap();

    let version_directory = temp.path().join("version-tamper");
    let (version_record, version_path) =
        write_immutable_sync_evidence(&version_directory, &provider_api_evidence()).unwrap();
    let mut version_json = serde_json::to_value(&version_record).unwrap();
    version_json["version"] = serde_json::json!(version_record.version + 1);
    make_writable(&version_path);
    std::fs::write(&version_path, serde_json::to_vec_pretty(&version_json).unwrap()).unwrap();
    make_read_only(&version_path);
    assert_eq!(
        read_immutable_sync_evidence(&version_path).unwrap_err(),
        "provider-evidence-record-version-unsupported"
    );

    let digest_directory = temp.path().join("digest-tamper");
    let (digest_record, digest_path) =
        write_immutable_sync_evidence(&digest_directory, &provider_api_evidence()).unwrap();
    let mut digest_json = serde_json::to_value(&digest_record).unwrap();
    digest_json["evidence"]["observed_bytes"] = serde_json::json!(43);
    make_writable(&digest_path);
    std::fs::write(&digest_path, serde_json::to_vec_pretty(&digest_json).unwrap()).unwrap();
    make_read_only(&digest_path);
    assert_eq!(
        read_immutable_sync_evidence(&digest_path).unwrap_err(),
        "provider-evidence-record-integrity-mismatch"
    );

    let shape_directory = temp.path().join("shape-tamper");
    let (shape_record, shape_path) =
        write_immutable_sync_evidence(&shape_directory, &provider_api_evidence()).unwrap();
    let mut shape_json = serde_json::to_value(&shape_record).unwrap();
    shape_json["evidence"]["destination_blake3"] = serde_json::json!("z".repeat(64));
    make_writable(&shape_path);
    std::fs::write(&shape_path, serde_json::to_vec_pretty(&shape_json).unwrap()).unwrap();
    make_read_only(&shape_path);
    assert_eq!(
        read_immutable_sync_evidence(&shape_path).unwrap_err(),
        "provider-evidence-destination-hash-invalid"
    );
}

#[cfg(unix)]
#[test]
fn immutable_provider_evidence_rejects_symlink_directory_and_symlink_record() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let real_directory = temp.path().join("real-provider-evidence");
    std::fs::create_dir(&real_directory).unwrap();
    let symlink_directory = temp.path().join("provider-evidence-link");
    symlink(&real_directory, &symlink_directory).unwrap();

    assert_eq!(
        write_immutable_sync_evidence(&symlink_directory, &provider_api_evidence()).unwrap_err(),
        "provider-evidence-directory-unsafe"
    );

    let regular = temp.path().join("regular.json");
    std::fs::write(&regular, b"{}").unwrap();
    make_read_only(&regular);
    let symlink_record = temp.path().join("record-link.json");
    symlink(&regular, &symlink_record).unwrap();
    assert_eq!(
        read_immutable_sync_evidence(&symlink_record).unwrap_err(),
        "provider-evidence-record-must-be-read-only-regular-file"
    );
}
