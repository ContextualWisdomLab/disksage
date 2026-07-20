//! Durable, integrity-bound records for provider synchronization evidence.
//!
//! Provider status is time-sensitive. A successful check must therefore be persisted before a
//! later source-eviction step can proceed, rather than surviving only in terminal or UI output.

use crate::cloud_transfer::{ProviderSyncEvidence, SyncEvidenceKind};
use std::path::Path;

#[cfg(not(coverage))]
use std::io::{Read, Write};
#[cfg(not(coverage))]
use std::path::PathBuf;

pub const PROVIDER_EVIDENCE_RECORD_VERSION: u32 = 1;
#[cfg(not(coverage))]
const MAX_PROVIDER_EVIDENCE_RECORD_BYTES: u64 = 64 * 1024;
const MAX_EVIDENCE_ID_BYTES: usize = 1_024;
const MAX_DESTINATION_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSyncEvidenceRecord {
    pub version: u32,
    pub record_id: String,
    pub evidence: ProviderSyncEvidence,
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn validate_evidence(evidence: &ProviderSyncEvidence) -> Result<(), String> {
    if !valid_hex64(&evidence.receipt_id) {
        return Err("provider-evidence-receipt-id-invalid".into());
    }
    if evidence.destination.is_empty()
        || evidence.destination.len() > MAX_DESTINATION_BYTES
        || !absolute_without_parent(Path::new(&evidence.destination))
    {
        return Err("provider-evidence-destination-invalid".into());
    }
    if !valid_hex64(&evidence.destination_blake3) {
        return Err("provider-evidence-destination-hash-invalid".into());
    }
    if evidence.evidence_id.is_empty()
        || evidence.evidence_id.len() > MAX_EVIDENCE_ID_BYTES
        || evidence.evidence_id.chars().any(char::is_control)
    {
        return Err("provider-evidence-id-invalid".into());
    }
    match (evidence.kind, &evidence.remote_content) {
        (SyncEvidenceKind::ProviderNativeStatus, None)
        | (SyncEvidenceKind::ProviderApi, Some(_)) => Ok(()),
        (SyncEvidenceKind::ProviderNativeStatus, Some(_)) => {
            Err("provider-evidence-native-remote-content-unexpected".into())
        }
        (SyncEvidenceKind::ProviderApi, None) => {
            Err("provider-evidence-api-remote-content-missing".into())
        }
    }
}

fn record_id_for(version: u32, evidence: &ProviderSyncEvidence) -> Result<String, String> {
    let encoded =
        serde_json::to_vec(evidence).map_err(|_| "provider-evidence-json-invalid".to_string())?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-provider-sync-evidence-record\0");
    hasher.update(&version.to_le_bytes());
    hasher.update(&(encoded.len() as u64).to_le_bytes());
    hasher.update(&encoded);
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn create_sync_evidence_record(
    evidence: &ProviderSyncEvidence,
) -> Result<ProviderSyncEvidenceRecord, String> {
    validate_evidence(evidence)?;
    let record_id = record_id_for(PROVIDER_EVIDENCE_RECORD_VERSION, evidence)?;
    Ok(ProviderSyncEvidenceRecord {
        version: PROVIDER_EVIDENCE_RECORD_VERSION,
        record_id,
        evidence: evidence.clone(),
    })
}

pub fn validate_sync_evidence_record(record: &ProviderSyncEvidenceRecord) -> Result<(), String> {
    if record.version != PROVIDER_EVIDENCE_RECORD_VERSION {
        return Err("provider-evidence-record-version-unsupported".into());
    }
    validate_evidence(&record.evidence)?;
    if !valid_hex64(&record.record_id)
        || record.record_id != record_id_for(record.version, &record.evidence)?
    {
        return Err("provider-evidence-record-integrity-mismatch".into());
    }
    Ok(())
}

#[cfg(not(coverage))]
fn record_filename(record: &ProviderSyncEvidenceRecord) -> String {
    format!(
        "{}-{:020}-{}.json",
        record.evidence.receipt_id, record.evidence.confirmed_at_ms, record.record_id
    )
}

#[cfg(not(coverage))]
fn secure_evidence_directory(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path)
        .map_err(|_| "provider-evidence-directory-create-failed".to_string())?;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "provider-evidence-directory-metadata-failed".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("provider-evidence-directory-unsafe".into());
    }
    Ok(())
}

/// Persist the full provider claim before it is used to authorize source eviction.
///
/// The file is create-only, read-only, fsynced, and named by the receipt, observation time, and
/// integrity digest. Existing evidence is never overwritten.
#[cfg(not(coverage))]
pub fn write_immutable_sync_evidence(
    directory: &Path,
    evidence: &ProviderSyncEvidence,
) -> Result<(ProviderSyncEvidenceRecord, PathBuf), String> {
    let record = create_sync_evidence_record(evidence)?;
    secure_evidence_directory(directory)?;
    let path = directory.join(record_filename(&record));
    let encoded = serde_json::to_vec_pretty(&record)
        .map_err(|_| "provider-evidence-json-invalid".to_string())?;
    if encoded.len() as u64 > MAX_PROVIDER_EVIDENCE_RECORD_BYTES {
        return Err("provider-evidence-record-too-large".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| "provider-evidence-record-create-failed".to_string())?;
    let result = (|| -> Result<(), String> {
        file.write_all(&encoded)
            .and_then(|_| file.sync_all())
            .map_err(|_| "provider-evidence-record-write-failed".to_string())?;
        let mut permissions = file
            .metadata()
            .map_err(|_| "provider-evidence-record-metadata-failed".to_string())?
            .permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o400);
        }
        #[cfg(not(unix))]
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions)
            .map_err(|_| "provider-evidence-record-permissions-failed".to_string())?;
        #[cfg(unix)]
        std::fs::File::open(directory)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| "provider-evidence-directory-sync-failed".to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    Ok((record, path))
}

#[cfg(not(coverage))]
fn same_file_identity(expected: &std::fs::Metadata, observed: &std::fs::Metadata) -> bool {
    let common = expected.file_type().is_file()
        && observed.file_type().is_file()
        && !expected.file_type().is_symlink()
        && !observed.file_type().is_symlink()
        && expected.len() == observed.len()
        && expected.permissions().readonly()
        && observed.permissions().readonly()
        && expected.modified().ok() == observed.modified().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        common && expected.dev() == observed.dev() && expected.ino() == observed.ino()
    }
    #[cfg(not(unix))]
    {
        common
    }
}

/// Read a bounded immutable evidence record and verify both its identity-bound filename and digest.
#[cfg(not(coverage))]
pub fn read_immutable_sync_evidence(path: &Path) -> Result<ProviderSyncEvidenceRecord, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "provider-evidence-record-metadata-failed".to_string())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || !metadata.permissions().readonly()
    {
        return Err("provider-evidence-record-must-be-read-only-regular-file".into());
    }
    if metadata.len() > MAX_PROVIDER_EVIDENCE_RECORD_BYTES {
        return Err("provider-evidence-record-too-large".into());
    }
    let mut file = std::fs::File::open(path)
        .map_err(|_| "provider-evidence-record-open-failed".to_string())?;
    let opened = file
        .metadata()
        .map_err(|_| "provider-evidence-record-metadata-failed".to_string())?;
    if !same_file_identity(&metadata, &opened) {
        return Err("provider-evidence-record-changed-during-read".into());
    }
    let mut encoded = Vec::with_capacity(metadata.len() as usize);
    Read::by_ref(&mut file)
        .take(MAX_PROVIDER_EVIDENCE_RECORD_BYTES + 1)
        .read_to_end(&mut encoded)
        .map_err(|_| "provider-evidence-record-read-failed".to_string())?;
    if encoded.len() as u64 > MAX_PROVIDER_EVIDENCE_RECORD_BYTES {
        return Err("provider-evidence-record-too-large".into());
    }
    let after = std::fs::symlink_metadata(path)
        .map_err(|_| "provider-evidence-record-metadata-failed".to_string())?;
    if !same_file_identity(&metadata, &after) {
        return Err("provider-evidence-record-changed-during-read".into());
    }
    let record: ProviderSyncEvidenceRecord = serde_json::from_slice(&encoded)
        .map_err(|_| "provider-evidence-record-json-invalid".to_string())?;
    validate_sync_evidence_record(&record)?;
    if path.file_name().and_then(|name| name.to_str()) != Some(record_filename(&record).as_str()) {
        return Err("provider-evidence-record-filename-id-mismatch".into());
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::CloudProvider;
    use crate::cloud_transfer::{RemoteChecksumAlgorithm, RemoteContentProof};

    fn evidence() -> ProviderSyncEvidence {
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

    #[test]
    fn record_digest_binds_every_serialized_evidence_field() {
        let record = create_sync_evidence_record(&evidence()).unwrap();
        validate_sync_evidence_record(&record).unwrap();

        let mut changed = record.clone();
        changed.evidence.confirmed_at_ms += 1;
        assert_eq!(
            validate_sync_evidence_record(&changed).unwrap_err(),
            "provider-evidence-record-integrity-mismatch"
        );
    }

    #[test]
    fn record_rejects_unsafe_or_incomplete_shapes() {
        let mut unsafe_evidence = evidence();
        unsafe_evidence.receipt_id = "../receipt".into();
        assert!(create_sync_evidence_record(&unsafe_evidence).is_err());

        let mut native_with_remote = evidence();
        native_with_remote.kind = SyncEvidenceKind::ProviderNativeStatus;
        assert!(create_sync_evidence_record(&native_with_remote).is_err());

        let mut api_without_remote = evidence();
        api_without_remote.remote_content = None;
        assert!(create_sync_evidence_record(&api_without_remote).is_err());

        let record = create_sync_evidence_record(&evidence()).unwrap();
        let mut value = serde_json::to_value(record).unwrap();
        value["evidence"]["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ProviderSyncEvidenceRecord>(value).is_err());
    }

    #[cfg(not(coverage))]
    #[test]
    fn immutable_record_round_trip_rejects_rename_and_collision() {
        let temp = tempfile::tempdir().unwrap();
        let (record, path) = write_immutable_sync_evidence(temp.path(), &evidence()).unwrap();
        assert_eq!(read_immutable_sync_evidence(&path).unwrap(), record);
        assert!(write_immutable_sync_evidence(temp.path(), &evidence()).is_err());

        let renamed = temp.path().join("wrong.json");
        std::fs::rename(&path, &renamed).unwrap();
        assert_eq!(
            read_immutable_sync_evidence(&renamed).unwrap_err(),
            "provider-evidence-record-filename-id-mismatch"
        );
    }
}
