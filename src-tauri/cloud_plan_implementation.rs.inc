//! Headless entrypoint for planning, reviewing, copying, and attesting cloud archive candidates.

#[cfg(target_os = "macos")]
embed_plist::embed_info_plist!("../../disksage-cloud-plan.Info.plist");

#[cfg(not(coverage))]
use std::collections::BTreeMap;
#[cfg(all(not(coverage), unix))]
use std::fs::OpenOptions;
#[cfg(all(not(coverage), unix))]
use std::io::Write;
#[cfg(not(coverage))]
use std::path::{Path, PathBuf};
#[cfg(not(coverage))]
use std::sync::atomic::AtomicBool;
#[cfg(not(coverage))]
use std::time::{Duration, Instant};

#[cfg(not(coverage))]
const PROVIDER_API_COPY_DEADLINE_SECS: u64 = 30 * 60;

#[cfg(not(coverage))]
use disksage_lib::cloud::{
    self, ArchiveKind, CloudAccountScope, CloudPlanOptions, CloudProvider, CloudRoot,
};
#[cfg(not(coverage))]
use disksage_lib::cloud_adr;
#[cfg(not(coverage))]
use disksage_lib::cloud_eviction::{self, CloudEvictionResult, CloudSourceEvictionApproval};
#[cfg(not(coverage))]
use disksage_lib::cloud_local_eviction;
#[cfg(not(coverage))]
use disksage_lib::cloud_review::{self, CloudReviewDecision, CloudReviewDisposition};
#[cfg(not(coverage))]
use disksage_lib::cloud_transfer::{self, CloudCopyReceipt, LocalEvictionPermit};
#[cfg(not(coverage))]
use disksage_lib::icloud_sync_health;
#[cfg(not(coverage))]
use disksage_lib::naruon_capacity;
#[cfg(not(coverage))]
use disksage_lib::naruon_cloud_copy_readiness;
use disksage_lib::naruon_lineage;
#[cfg(not(coverage))]
use disksage_lib::provider_api_client::{self, FixedHostProviderMetadataClient};
#[cfg(not(coverage))]
use disksage_lib::provider_api_write;
#[cfg(not(coverage))]
use disksage_lib::provider_capacity::{self, FixedHostProviderCapacityClient};
#[cfg(not(coverage))]
use disksage_lib::provider_client_runtime;
#[cfg(not(coverage))]
use disksage_lib::provider_evidence::{self, ProviderSyncEvidenceRecord};
#[cfg(not(coverage))]
use disksage_lib::provider_global_sync;
#[cfg(not(coverage))]
use disksage_lib::provider_oauth;
#[cfg(not(coverage))]
use disksage_lib::provider_sync;
#[cfg(not(coverage))]
use disksage_lib::semantic_catalog;
#[cfg(all(not(coverage), unix))]
use sha2::{Digest, Sha256};

#[cfg(not(coverage))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    root: PathBuf,
    cloud_root: Option<PathBuf>,
    provider: Option<CloudProvider>,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    list_roots: bool,
    inspect_roots: bool,
    all_readable_roots: bool,
    verify_capacity: bool,
    decision_summary: bool,
    review_reason_set: Option<Vec<String>>,
    private_review_output: Option<PathBuf>,
    private_candidate_inspection_output: Option<PathBuf>,
    exact_duplicate_review_prefix: Option<String>,
    exact_duplicate_kind: Option<ArchiveKind>,
    capacity_reserve_mib: u64,
    copy_fingerprint: Option<String>,
    provider_api_copy_fingerprint: Option<String>,
    adopt_existing_fingerprint: Option<String>,
    receipt_dir: Option<PathBuf>,
    audit_receipts: bool,
    reconcile_receipts: bool,
    confirm_copy_phrase: Option<String>,
    attest_receipt: Option<PathBuf>,
    evidence_dir: Option<PathBuf>,
    provider_object_id: Option<String>,
    oauth_connections: Option<PathBuf>,
    evict_receipt: Option<PathBuf>,
    confirm_receipt_id: Option<String>,
    eviction_dir: Option<PathBuf>,
    eviction_approval_dir: Option<PathBuf>,
    journal_path: Option<PathBuf>,
    review_candidate_fingerprint: Option<String>,
    review_fingerprint: Option<String>,
    review_disposition: Option<CloudReviewDisposition>,
    reviewed_by: Option<String>,
    review_rationale: Option<String>,
    review_dir: Option<PathBuf>,
    export_naruon_lineage: Option<PathBuf>,
    naruon_sync_evidence: Option<PathBuf>,
    export_naruon_capacity: bool,
    export_naruon_copy_readiness: bool,
    naruon_copy_readiness_output: Option<PathBuf>,
    export_semantic_catalog: bool,
}

#[cfg(not(coverage))]
fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

#[cfg(not(coverage))]
fn parse_provider(value: &str) -> Result<CloudProvider, String> {
    match value {
        "icloud" => Ok(CloudProvider::Icloud),
        "onedrive" => Ok(CloudProvider::Onedrive),
        "google-drive" => Ok(CloudProvider::GoogleDrive),
        _ => Err(format!("지원하지 않는 provider: {value}")),
    }
}

#[cfg(not(coverage))]
fn parse_review_reason_set(value: &str) -> Result<Vec<String>, String> {
    if value.len() > 2_048 {
        return Err("--review-reason-set 값이 너무 김".into());
    }
    let raw = value.split('|').collect::<Vec<_>>();
    if raw.is_empty() || raw.len() > 16 {
        return Err("--review-reason-set은 1개 이상 16개 이하 사유여야 함".into());
    }
    let mut reasons = Vec::with_capacity(raw.len());
    for reason in raw {
        if reason.is_empty()
            || reason.len() > 128
            || !reason
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err("--review-reason-set 사유 형식이 올바르지 않음".into());
        }
        reasons.push(reason.to_string());
    }
    let original_len = reasons.len();
    reasons.sort();
    reasons.dedup();
    if reasons.len() != original_len {
        return Err("--review-reason-set에 중복 사유가 있음".into());
    }
    Ok(reasons)
}

#[cfg(not(coverage))]
fn parse_exact_duplicate_review_prefix(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value.len() > 255
        || matches!(value, "." | "..")
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err("--exact-duplicate-review-prefix는 단일 디렉터리 이름 prefix여야 함".into());
    }
    Ok(value.to_string())
}

#[cfg(not(coverage))]
fn parse_archive_kind(value: &str) -> Result<ArchiveKind, String> {
    match value {
        "document" => Ok(ArchiveKind::Document),
        "media" => Ok(ArchiveKind::Media),
        "archive" => Ok(ArchiveKind::Archive),
        "dataset" => Ok(ArchiveKind::Dataset),
        "backup" => Ok(ArchiveKind::Backup),
        "creative" => Ok(ArchiveKind::Creative),
        "incomplete-download" => Ok(ArchiveKind::IncompleteDownload),
        _ => Err(format!("지원하지 않는 exact duplicate kind: {value}")),
    }
}

#[cfg(not(coverage))]
fn parse_args(args: &[String], home: &Path) -> Result<Args, String> {
    let mut parsed = Args {
        root: home.to_path_buf(),
        cloud_root: None,
        provider: None,
        min_size_mib: 256,
        min_age_days: 90,
        limit: 200,
        list_roots: false,
        inspect_roots: false,
        all_readable_roots: false,
        verify_capacity: false,
        decision_summary: false,
        review_reason_set: None,
        private_review_output: None,
        private_candidate_inspection_output: None,
        exact_duplicate_review_prefix: None,
        exact_duplicate_kind: None,
        capacity_reserve_mib: 1024,
        copy_fingerprint: None,
        provider_api_copy_fingerprint: None,
        adopt_existing_fingerprint: None,
        receipt_dir: None,
        audit_receipts: false,
        reconcile_receipts: false,
        confirm_copy_phrase: None,
        attest_receipt: None,
        evidence_dir: None,
        provider_object_id: None,
        oauth_connections: None,
        evict_receipt: None,
        confirm_receipt_id: None,
        eviction_dir: None,
        eviction_approval_dir: None,
        journal_path: None,
        review_candidate_fingerprint: None,
        review_fingerprint: None,
        review_disposition: None,
        reviewed_by: None,
        review_rationale: None,
        review_dir: None,
        export_naruon_lineage: None,
        naruon_sync_evidence: None,
        export_naruon_capacity: false,
        export_naruon_copy_readiness: false,
        naruon_copy_readiness_output: None,
        export_semantic_catalog: false,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => parsed.root = PathBuf::from(value(args, &mut index, "--root")?),
            "--cloud-root" => {
                parsed.cloud_root = Some(PathBuf::from(value(args, &mut index, "--cloud-root")?))
            }
            "--provider" => {
                parsed.provider = Some(parse_provider(&value(args, &mut index, "--provider")?)?)
            }
            "--min-size-mib" => {
                parsed.min_size_mib = value(args, &mut index, "--min-size-mib")?
                    .parse()
                    .map_err(|_| "--min-size-mib는 정수여야 함".to_string())?
            }
            "--min-age-days" => {
                parsed.min_age_days = value(args, &mut index, "--min-age-days")?
                    .parse()
                    .map_err(|_| "--min-age-days는 정수여야 함".to_string())?
            }
            "--limit" => {
                parsed.limit = value(args, &mut index, "--limit")?
                    .parse()
                    .map_err(|_| "--limit는 정수여야 함".to_string())?
            }
            "--list-roots" => parsed.list_roots = true,
            "--inspect-roots" => parsed.inspect_roots = true,
            "--all-readable-roots" => parsed.all_readable_roots = true,
            "--verify-capacity" => parsed.verify_capacity = true,
            "--decision-summary" => parsed.decision_summary = true,
            "--review-reason-set" => {
                if parsed.review_reason_set.is_some() {
                    return Err("--review-reason-set은 한 번만 지정할 수 있음".into());
                }
                parsed.review_reason_set = Some(parse_review_reason_set(&value(
                    args,
                    &mut index,
                    "--review-reason-set",
                )?)?);
            }
            "--private-review-output" => {
                if parsed.private_review_output.is_some() {
                    return Err("--private-review-output은 한 번만 지정할 수 있음".into());
                }
                parsed.private_review_output = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--private-review-output",
                )?));
            }
            "--private-candidate-inspection-output" => {
                if parsed.private_candidate_inspection_output.is_some() {
                    return Err(
                        "--private-candidate-inspection-output은 한 번만 지정할 수 있음"
                            .into(),
                    );
                }
                parsed.private_candidate_inspection_output = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--private-candidate-inspection-output",
                )?));
            }
            "--exact-duplicate-review-prefix" => {
                if parsed.exact_duplicate_review_prefix.is_some() {
                    return Err(
                        "--exact-duplicate-review-prefix는 한 번만 지정할 수 있음".into(),
                    );
                }
                parsed.exact_duplicate_review_prefix = Some(
                    parse_exact_duplicate_review_prefix(&value(
                        args,
                        &mut index,
                        "--exact-duplicate-review-prefix",
                    )?)?,
                );
            }
            "--exact-duplicate-kind" => {
                if parsed.exact_duplicate_kind.is_some() {
                    return Err("--exact-duplicate-kind는 한 번만 지정할 수 있음".into());
                }
                parsed.exact_duplicate_kind = Some(parse_archive_kind(&value(
                    args,
                    &mut index,
                    "--exact-duplicate-kind",
                )?)?);
            }
            "--capacity-reserve-mib" => {
                parsed.capacity_reserve_mib = value(args, &mut index, "--capacity-reserve-mib")?
                    .parse()
                    .map_err(|_| "--capacity-reserve-mib는 정수여야 함".to_string())?
            }
            "--copy-fingerprint" => {
                parsed.copy_fingerprint = Some(value(args, &mut index, "--copy-fingerprint")?)
            }
            "--provider-api-copy-fingerprint" => {
                parsed.provider_api_copy_fingerprint = Some(value(
                    args,
                    &mut index,
                    "--provider-api-copy-fingerprint",
                )?)
            }
            "--adopt-existing-fingerprint" => {
                parsed.adopt_existing_fingerprint = Some(value(
                    args,
                    &mut index,
                    "--adopt-existing-fingerprint",
                )?)
            }
            "--receipt-dir" => {
                parsed.receipt_dir = Some(PathBuf::from(value(args, &mut index, "--receipt-dir")?))
            }
            "--audit-receipts" => parsed.audit_receipts = true,
            "--reconcile-receipts" => parsed.reconcile_receipts = true,
            "--confirm-copy-phrase" => {
                parsed.confirm_copy_phrase =
                    Some(value(args, &mut index, "--confirm-copy-phrase")?)
            }
            "--attest-receipt" => {
                parsed.attest_receipt = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--attest-receipt",
                )?))
            }
            "--evidence-dir" => {
                parsed.evidence_dir = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--evidence-dir",
                )?))
            }
            "--provider-object-id" => {
                parsed.provider_object_id = Some(value(args, &mut index, "--provider-object-id")?)
            }
            "--oauth-connections" => {
                parsed.oauth_connections = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--oauth-connections",
                )?))
            }
            "--evict-receipt" => {
                parsed.evict_receipt = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--evict-receipt",
                )?))
            }
            "--confirm-receipt-id" => {
                parsed.confirm_receipt_id =
                    Some(value(args, &mut index, "--confirm-receipt-id")?)
            }
            "--eviction-dir" => {
                parsed.eviction_dir =
                    Some(PathBuf::from(value(args, &mut index, "--eviction-dir")?))
            }
            "--eviction-approval-dir" => {
                parsed.eviction_approval_dir = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--eviction-approval-dir",
                )?))
            }
            "--journal-path" => {
                parsed.journal_path =
                    Some(PathBuf::from(value(args, &mut index, "--journal-path")?))
            }
            "--review-candidate-fingerprint" => {
                parsed.review_candidate_fingerprint = Some(value(
                    args,
                    &mut index,
                    "--review-candidate-fingerprint",
                )?)
            }
            "--review-fingerprint" => {
                parsed.review_fingerprint =
                    Some(value(args, &mut index, "--review-fingerprint")?)
            }
            "--review-disposition" => {
                parsed.review_disposition = Some(match value(
                    args,
                    &mut index,
                    "--review-disposition",
                )?
                .as_str()
                {
                    "approved" => CloudReviewDisposition::Approved,
                    "held" => CloudReviewDisposition::Held,
                    value => return Err(format!("지원하지 않는 review disposition: {value}")),
                })
            }
            "--reviewed-by" => {
                parsed.reviewed_by = Some(value(args, &mut index, "--reviewed-by")?)
            }
            "--review-rationale" => {
                parsed.review_rationale = Some(value(args, &mut index, "--review-rationale")?)
            }
            "--review-dir" => {
                parsed.review_dir =
                    Some(PathBuf::from(value(args, &mut index, "--review-dir")?))
            }
            "--export-naruon-lineage" => {
                parsed.export_naruon_lineage = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--export-naruon-lineage",
                )?))
            }
            "--naruon-sync-evidence" => {
                parsed.naruon_sync_evidence = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--naruon-sync-evidence",
                )?))
            }
            "--export-naruon-capacity" => parsed.export_naruon_capacity = true,
            "--export-naruon-copy-readiness" => {
                parsed.export_naruon_copy_readiness = true
            }
            "--naruon-copy-readiness-output" => {
                if parsed.naruon_copy_readiness_output.is_some() {
                    return Err(
                        "--naruon-copy-readiness-output은 한 번만 지정할 수 있음"
                            .into(),
                    );
                }
                parsed.naruon_copy_readiness_output =
                    Some(PathBuf::from(value(
                        args,
                        &mut index,
                        "--naruon-copy-readiness-output",
                    )?));
            }
            "--export-semantic-catalog" => parsed.export_semantic_catalog = true,
            "--help" | "-h" => {
                return Err(
                    "usage: disksage-cloud-plan [--list-roots | --inspect-roots] [--root PATH] [--cloud-root PATH | --provider icloud|onedrive|google-drive | --all-readable-roots --decision-summary] [--min-size-mib N] [--min-age-days N] [--limit N] [--audit-receipts --receipt-dir ABSOLUTE_PATH] [--reconcile-receipts --receipt-dir ABSOLUTE_PATH --evidence-dir ABSOLUTE_PATH [--oauth-connections ABSOLUTE_PATH [--provider-object-id GOOGLE_FILE_ID]]] [--decision-summary [--private-candidate-inspection-output ABSOLUTE_NEW_FILE.json | --review-reason-set REASON|REASON [--private-review-output ABSOLUTE_NEW_FILE.json]] | --exact-duplicate-review-prefix DIR_PREFIX --exact-duplicate-kind document|media|archive|dataset|backup|creative|incomplete-download | --export-naruon-copy-readiness --verify-capacity [--naruon-copy-readiness-output ABSOLUTE_NEW_FILE.json] | --export-semantic-catalog] [--verify-capacity [--oauth-connections ABSOLUTE_PATH] [--export-naruon-capacity]] [--capacity-reserve-mib N] [--copy-fingerprint HEX64 --receipt-dir PATH --confirm-copy-phrase EXACT --reviewed-by human:ID --review-rationale TEXT [--review-dir PATH] [--oauth-connections ABSOLUTE_PATH] | --provider-api-copy-fingerprint HEX64 --receipt-dir PATH --oauth-connections ABSOLUTE_PATH --confirm-copy-phrase EXACT --reviewed-by human:ID --review-rationale TEXT [--review-dir PATH] | --adopt-existing-fingerprint HEX64 --receipt-dir PATH --confirm-copy-phrase EXACT --reviewed-by human:ID --review-rationale TEXT [--review-dir PATH] | --attest-receipt RECEIPT.json --evidence-dir ABSOLUTE_PATH [--oauth-connections ABSOLUTE_PATH [--provider-object-id GOOGLE_FILE_ID]] | --evict-receipt RECEIPT.json --confirm-receipt-id HEX64 --eviction-dir ABSOLUTE_PATH --eviction-approval-dir ABSOLUTE_PATH --journal-path ABSOLUTE_PATH --evidence-dir ABSOLUTE_PATH --reviewed-by human:ID --review-rationale TEXT [--oauth-connections ABSOLUTE_PATH [--provider-object-id GOOGLE_FILE_ID]] | --review-candidate-fingerprint HEX64 --review-fingerprint HEX64 --review-disposition approved|held --reviewed-by human:ID --review-rationale TEXT --review-dir PATH | --export-naruon-lineage RECEIPT.json [--naruon-sync-evidence EVIDENCE.json]]".into(),
                )
            }
            flag => return Err(format!("알 수 없는 인자: {flag}")),
        }
        index += 1;
    }
    Ok(parsed)
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct CopyOutput {
    action: &'static str,
    goal_state: cloud_transfer::CloudOffloadGoalState,
    goal_status: Option<String>,
    receipt: CloudCopyReceipt,
    receipt_path: String,
    adr_path: Option<String>,
    goal_path: Option<String>,
    projection_warnings: Vec<String>,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct ProviderApiCopyOutput {
    action: &'static str,
    goal_state: cloud_transfer::CloudOffloadGoalState,
    goal_status: Option<String>,
    receipt: CloudCopyReceipt,
    receipt_path: String,
    provider_object_id: String,
    evidence_path: Option<String>,
    adr_path: Option<String>,
    goal_path: Option<String>,
    projection_warnings: Vec<String>,
    permit: Option<LocalEvictionPermit>,
    blockers: Vec<String>,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct AttestationOutput {
    action: &'static str,
    goal_state: cloud_transfer::CloudOffloadGoalState,
    goal_status: Option<String>,
    receipt_id: String,
    evidence: disksage_lib::cloud_transfer::ProviderSyncEvidence,
    assessment: provider_sync::ProviderSyncTimelinessAssessment,
    evidence_record: ProviderSyncEvidenceRecord,
    evidence_path: String,
    adr_path: Option<String>,
    goal_path: Option<String>,
    projection_warnings: Vec<String>,
    permit: Option<LocalEvictionPermit>,
    blockers: Vec<String>,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct EvictionOutput {
    action: &'static str,
    goal_state: cloud_transfer::CloudOffloadGoalState,
    receipt_id: String,
    evidence: disksage_lib::cloud_transfer::ProviderSyncEvidence,
    evidence_record: ProviderSyncEvidenceRecord,
    evidence_path: String,
    permit: LocalEvictionPermit,
    approval: CloudSourceEvictionApproval,
    approval_path: String,
    eviction: CloudEvictionResult,
    adr_path: Option<String>,
    goal_path: Option<String>,
    projection_warnings: Vec<String>,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct ReviewOutput {
    action: &'static str,
    decision: CloudReviewDecision,
    decision_path: String,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct ReceiptReconciliationEntry {
    file_name: String,
    receipt_id: Option<String>,
    provider: Option<CloudProvider>,
    bytes: Option<u64>,
    source_state: Option<String>,
    destination_state: Option<String>,
    adr_projection_state: Option<String>,
    goal_projection_state: Option<String>,
    goal_status: Option<String>,
    goal_state: Option<cloud_transfer::CloudOffloadGoalState>,
    provider_sync_state: Option<cloud_transfer::ProviderSyncState>,
    eviction_permit: bool,
    attestation_error: Option<String>,
    evidence_record_count: u64,
    issues: Vec<String>,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct ReceiptReconciliationReport {
    schema_version: u32,
    output_mode: &'static str,
    generated_at_ms: u64,
    receipts_seen: u64,
    valid_receipts: u64,
    invalid_receipts: u64,
    ignored_entries: u64,
    source_not_present_count: u64,
    destination_not_present_count: u64,
    source_missing_destination_present_count: u64,
    incomplete_projection_count: u64,
    attestation_attempted_count: u64,
    provider_evidence_written_count: u64,
    pending_provider_sync_count: u64,
    eviction_ready_count: u64,
    unprocessed_count: u64,
    incomplete_reconciliation: bool,
    entries: Vec<ReceiptReconciliationEntry>,
    mutation_performed: bool,
    cloud_write_executed: bool,
    source_eviction_authorized: bool,
    notices: Vec<&'static str>,
}

#[cfg(not(coverage))]
const MAX_RECONCILIATION_RECEIPTS: usize = 10_000;
#[cfg(not(coverage))]
const MAX_RECONCILIATION_ATTESTATIONS: usize = 256;
#[cfg(not(coverage))]
const RECONCILIATION_MAX_DURATION: Duration = Duration::from_secs(30);

#[cfg(not(coverage))]
const MAX_RECONCILIATION_PROJECTION_BYTES: u64 = 64 * 1024;

#[cfg(not(coverage))]
fn regular_file_state(path: &Path) -> &'static str {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => "unsafe",
        Ok(metadata) if metadata.is_file() => "present",
        Ok(_) => "unsafe",
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "missing",
        Err(_) => "unavailable",
    }
}

#[cfg(not(coverage))]
fn projection_state(path: &Path, kind: &str, receipt_id: &str) -> &'static str {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return "missing",
        Err(_) => return "unavailable",
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return "unsafe";
    }
    if metadata.len() > MAX_RECONCILIATION_PROJECTION_BYTES {
        return "oversized";
    }
    let encoded = match std::fs::read(path) {
        Ok(encoded) => encoded,
        Err(_) => return "unavailable",
    };
    match kind {
        "adr" => match serde_json::from_slice::<cloud_adr::CloudOffloadAdrSnapshot>(&encoded) {
            Ok(snapshot)
                if snapshot.schema_version == cloud_adr::CLOUD_ADR_SCHEMA_VERSION
                    && snapshot.receipt_id == receipt_id
                    && snapshot.adr_id == format!("cloud-offload:{receipt_id}") =>
            {
                "valid"
            }
            Ok(snapshot) if snapshot.receipt_id != receipt_id => "invalid-binding",
            Ok(_) => "invalid-schema",
            Err(_) => "invalid",
        },
        "goal" => match serde_json::from_slice::<cloud_adr::CloudOffloadGoalSnapshot>(&encoded) {
            Ok(snapshot)
                if snapshot.schema_version == cloud_adr::CLOUD_GOAL_SCHEMA_VERSION
                    && snapshot.receipt_id == receipt_id
                    && snapshot.goal_id == "disksage-cloud-offload" =>
            {
                "valid"
            }
            Ok(snapshot) if snapshot.receipt_id != receipt_id => "invalid-binding",
            Ok(_) => "invalid-schema",
            Err(_) => "invalid",
        },
        _ => "invalid",
    }
}

#[cfg(not(coverage))]
fn refresh_projection_issues(entry: &mut ReceiptReconciliationEntry) {
    entry.issues.retain(|issue| {
        !issue.starts_with("adr-projection-") && !issue.starts_with("goal-projection-")
    });
    for (kind, state) in [
        ("adr", entry.adr_projection_state.as_deref()),
        ("goal", entry.goal_projection_state.as_deref()),
    ] {
        if let Some(state) = state.filter(|state| *state != "valid") {
            entry.issues.push(format!("{kind}-projection-{state}"));
        }
    }
}

#[cfg(not(coverage))]
fn evidence_record_count(evidence_dirs: &[PathBuf], receipt_id: &str) -> u64 {
    let prefix = format!("{receipt_id}-");
    let mut names = BTreeMap::new();
    for evidence_dir in evidence_dirs {
        let Ok(entries) = std::fs::read_dir(evidence_dir) else {
            continue;
        };
        for entry in entries
            .filter_map(Result::ok)
            .take(MAX_RECONCILIATION_RECEIPTS)
        {
            let path = entry.path();
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name.starts_with(&prefix)
                && name.ends_with(".json")
                && regular_file_state(&path) == "present"
            {
                names.insert(name, ());
                if names.len() >= MAX_RECONCILIATION_RECEIPTS {
                    return names.len() as u64;
                }
            }
        }
    }
    names.len() as u64
}

#[cfg(not(coverage))]
fn audit_evidence_dirs(receipt_dir: &Path, evidence_dir: Option<&Path>) -> Vec<PathBuf> {
    if let Some(evidence_dir) = evidence_dir {
        return vec![evidence_dir.to_path_buf()];
    }
    let parent = receipt_dir.parent().unwrap_or(receipt_dir);
    let provider_dir = parent.join("cloud-provider-evidence");
    let legacy_dir = parent.join("cloud-sync-evidence");
    let legacy_is_safe_directory = std::fs::symlink_metadata(&legacy_dir)
        .map(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
        .unwrap_or(false);
    if legacy_is_safe_directory {
        vec![provider_dir, legacy_dir]
    } else {
        vec![provider_dir]
    }
}

#[cfg(not(coverage))]
fn audit_receipts(
    receipt_dir: &Path,
    evidence_dir: Option<&Path>,
    generated_at_ms: u64,
) -> Result<ReceiptReconciliationReport, String> {
    let metadata = std::fs::symlink_metadata(receipt_dir)
        .map_err(|_| "receipt-directory-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("receipt-directory-unsafe".into());
    }
    let mut paths = std::fs::read_dir(receipt_dir)
        .map_err(|_| "receipt-directory-read-failed".to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    if paths.len() > MAX_RECONCILIATION_RECEIPTS {
        return Err("receipt-directory-entry-limit-exceeded".into());
    }
    let parent = receipt_dir.parent().unwrap_or(receipt_dir);
    let evidence_dirs = audit_evidence_dirs(receipt_dir, evidence_dir);
    let projection_anchor = evidence_dirs
        .first()
        .cloned()
        .unwrap_or_else(|| parent.join("cloud-provider-evidence"));
    let (adr_dir, goal_dir) = cloud_projection_dirs(&projection_anchor);
    let mut report = ReceiptReconciliationReport {
        schema_version: 1,
        output_mode: "cloud-receipt-reconciliation",
        generated_at_ms,
        receipts_seen: 0,
        valid_receipts: 0,
        invalid_receipts: 0,
        ignored_entries: 0,
        source_not_present_count: 0,
        destination_not_present_count: 0,
        source_missing_destination_present_count: 0,
        incomplete_projection_count: 0,
        attestation_attempted_count: 0,
        provider_evidence_written_count: 0,
        pending_provider_sync_count: 0,
        eviction_ready_count: 0,
        unprocessed_count: 0,
        incomplete_reconciliation: false,
        entries: Vec::new(),
        mutation_performed: false,
        cloud_write_executed: false,
        source_eviction_authorized: false,
        notices: vec![
            "read-only",
            "immutable-receipts-remain-authority",
            "no-cloud-write",
            "no-local-eviction",
        ],
    };
    for path in paths {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        if regular_file_state(&path) != "present" || !file_name.ends_with(".json") {
            report.ignored_entries = report.ignored_entries.saturating_add(1);
            continue;
        }
        report.receipts_seen = report.receipts_seen.saturating_add(1);
        let receipt = match cloud_transfer::read_immutable_receipt(&path) {
            Ok(receipt) => receipt,
            Err(error) => {
                report.invalid_receipts = report.invalid_receipts.saturating_add(1);
                report.entries.push(ReceiptReconciliationEntry {
                    file_name,
                    receipt_id: None,
                    provider: None,
                    bytes: None,
                    source_state: None,
                    destination_state: None,
                    adr_projection_state: None,
                    goal_projection_state: None,
                    goal_status: None,
                    goal_state: None,
                    provider_sync_state: None,
                    eviction_permit: false,
                    attestation_error: None,
                    evidence_record_count: 0,
                    issues: vec![format!("receipt-invalid:{error}")],
                });
                continue;
            }
        };
        report.valid_receipts = report.valid_receipts.saturating_add(1);
        let source_state = regular_file_state(Path::new(&receipt.source));
        let destination_state = regular_file_state(Path::new(&receipt.destination));
        let adr_state = projection_state(
            &adr_dir.join(format!("{}-latest.json", receipt.receipt_id)),
            "adr",
            &receipt.receipt_id,
        );
        let goal_state = projection_state(
            &goal_dir.join(format!("{}-latest.json", receipt.receipt_id)),
            "goal",
            &receipt.receipt_id,
        );
        let mut issues = Vec::new();
        if source_state != "present" {
            report.source_not_present_count = report.source_not_present_count.saturating_add(1);
            issues.push(format!("source-{source_state}"));
        }
        if destination_state != "present" {
            report.destination_not_present_count =
                report.destination_not_present_count.saturating_add(1);
            issues.push(format!("destination-{destination_state}"));
        }
        if source_state == "missing" && destination_state == "present" {
            report.source_missing_destination_present_count = report
                .source_missing_destination_present_count
                .saturating_add(1);
            issues.push("source-missing-destination-present".into());
        }
        if adr_state != "valid" {
            issues.push(format!("adr-projection-{adr_state}"));
        }
        if goal_state != "valid" {
            issues.push(format!("goal-projection-{goal_state}"));
        }
        if adr_state != "valid" || goal_state != "valid" {
            report.incomplete_projection_count =
                report.incomplete_projection_count.saturating_add(1);
        }
        report.entries.push(ReceiptReconciliationEntry {
            file_name,
            receipt_id: Some(receipt.receipt_id.clone()),
            provider: Some(receipt.provider),
            bytes: Some(receipt.bytes),
            source_state: Some(source_state.into()),
            destination_state: Some(destination_state.into()),
            adr_projection_state: Some(adr_state.into()),
            goal_projection_state: Some(goal_state.into()),
            goal_status: cloud_adr::read_goal_status(&goal_dir, &receipt.receipt_id)
                .ok()
                .flatten(),
            goal_state: None,
            provider_sync_state: None,
            eviction_permit: false,
            attestation_error: None,
            evidence_record_count: evidence_record_count(&evidence_dirs, &receipt.receipt_id),
            issues,
        });
    }
    Ok(report)
}

#[cfg(not(coverage))]
fn attach_pre_copy_evidence_cohort(
    report: &mut cloud::CloudPlanReport,
    runtime: &provider_client_runtime::ProviderClientRuntimeSnapshot,
    health: Option<&icloud_sync_health::IcloudSyncHealthReport>,
) {
    let local = report
        .local_volume
        .as_ref()
        .map(|snapshot| cloud::PreCopyEvidenceObservation {
            stream: "volume-pressure-evidence".into(),
            observed_at_ms: snapshot.observed_at_ms,
            evidence_complete: disksage_lib::volume_pressure::validate_snapshot(snapshot).is_ok(),
            fingerprint: snapshot.evidence_fingerprint.clone(),
        })
        .unwrap_or_else(|| cloud::PreCopyEvidenceObservation {
            stream: "volume-pressure-evidence".into(),
            observed_at_ms: 0,
            evidence_complete: false,
            fingerprint: "0".repeat(64),
        });
    let runtime = cloud::PreCopyEvidenceObservation {
        stream: "provider-client-runtime-evidence".into(),
        observed_at_ms: runtime.observed_at_ms,
        evidence_complete: runtime.process_observation_complete,
        fingerprint: runtime.snapshot_fingerprint_sha256.clone(),
    };
    let health = health
        .and_then(|value| icloud_sync_health::health_evidence_snapshot_from_report(value).ok())
        .map(|snapshot| cloud::PreCopyEvidenceObservation {
            stream: "icloud-sync-health-evidence".into(),
            observed_at_ms: snapshot.observed_at_ms,
            evidence_complete: snapshot.evidence_complete,
            fingerprint: snapshot.evidence_fingerprint_sha256,
        })
        .unwrap_or_else(|| cloud::PreCopyEvidenceObservation {
            stream: "icloud-sync-health-evidence".into(),
            observed_at_ms: 0,
            evidence_complete: false,
            fingerprint: "0".repeat(64),
        });
    let cohort = cloud::compare_pre_copy_evidence(vec![local, runtime, health]);
    if cohort.complete {
        report.notices.push("pre-copy-evidence-cohort-complete".into());
    } else {
        report.notices.push("pre-copy-evidence-cohort-blocked".into());
        report.notices.extend(cohort.blockers.iter().cloned());
    }
    report.pre_copy_evidence = Some(cohort);
}

#[cfg(not(coverage))]
fn validate_action_args(args: &Args) -> Result<(), String> {
    let provider_api_copy_action = args.provider_api_copy_fingerprint.is_some();
    let copy_action = args.copy_fingerprint.is_some() || provider_api_copy_action;
    let adoption_action = args.adopt_existing_fingerprint.is_some();
    let exact_duplicate_review =
        args.exact_duplicate_review_prefix.is_some() || args.exact_duplicate_kind.is_some();
    if args.exact_duplicate_review_prefix.is_some() != args.exact_duplicate_kind.is_some() {
        return Err(
            "--exact-duplicate-review-prefix와 --exact-duplicate-kind는 함께 지정해야 함".into(),
        );
    }
    if args.all_readable_roots && !args.decision_summary {
        return Err("--all-readable-roots에는 --decision-summary가 필요함".into());
    }
    if args.all_readable_roots && (args.cloud_root.is_some() || args.provider.is_some()) {
        return Err(
            "--all-readable-roots는 --cloud-root 또는 --provider와 함께 사용할 수 없음".into(),
        );
    }
    if args.copy_fingerprint.is_some() && provider_api_copy_action {
        return Err("native copy와 provider API copy는 동시에 사용할 수 없음".into());
    }
    if copy_action && adoption_action {
        return Err("copy action과 existing-copy adoption action은 동시에 사용할 수 없음".into());
    }
    let receipt_audit_action = args.audit_receipts;
    let receipt_reconcile_action = args.reconcile_receipts;
    let receipt_action = receipt_audit_action || receipt_reconcile_action;
    if receipt_audit_action && receipt_reconcile_action {
        return Err("--audit-receipts와 --reconcile-receipts는 함께 사용할 수 없음".into());
    }
    if receipt_action && (copy_action || adoption_action) {
        return Err(
            "receipt audit/reconciliation은 copy/adoption action과 함께 사용할 수 없음".into(),
        );
    }
    if !receipt_action && (copy_action || adoption_action) != args.receipt_dir.is_some() {
        return Err("copy/adoption fingerprint와 --receipt-dir은 함께 지정해야 함".into());
    }
    if receipt_action && args.receipt_dir.is_none() {
        return Err("--audit-receipts/--reconcile-receipts에는 --receipt-dir이 필요함".into());
    }
    if receipt_reconcile_action && args.evidence_dir.is_none() {
        return Err("--reconcile-receipts에는 --evidence-dir이 필요함".into());
    }
    if (copy_action || adoption_action) != args.confirm_copy_phrase.is_some() {
        return Err("copy/adoption action에는 --confirm-copy-phrase가 반드시 필요함".into());
    }
    if provider_api_copy_action && args.oauth_connections.is_none() {
        return Err("--provider-api-copy-fingerprint에는 --oauth-connections가 필요함".into());
    }
    if provider_api_copy_action && args.provider_object_id.is_some() {
        return Err("provider API copy는 --provider-object-id를 직접 받을 수 없음".into());
    }
    let review_evidence_fields = [
        args.review_candidate_fingerprint.is_some(),
        args.review_fingerprint.is_some(),
        args.review_disposition.is_some(),
    ];
    if review_evidence_fields.iter().any(|value| *value)
        && !review_evidence_fields.iter().all(|value| *value)
    {
        return Err("review fingerprint와 disposition은 모두 함께 지정해야 함".into());
    }
    let attribution_fields = [args.reviewed_by.is_some(), args.review_rationale.is_some()];
    if attribution_fields.iter().any(|value| *value)
        && !attribution_fields.iter().all(|value| *value)
    {
        return Err("reviewer와 rationale는 함께 지정해야 함".into());
    }
    let attributed = attribution_fields.iter().all(|value| *value);
    let review_action = review_evidence_fields.iter().all(|value| *value);
    if review_action && !attributed {
        return Err("review action에는 reviewer와 rationale가 필요함".into());
    }
    if (copy_action || adoption_action) && !attributed {
        return Err("copy/adoption action에는 reviewer와 rationale가 필요함".into());
    }
    if attributed {
        cloud_review::validate_review_attribution(
            args.reviewed_by
                .as_deref()
                .ok_or_else(|| "--reviewed-by가 필요함".to_string())?,
            args.review_rationale
                .as_deref()
                .ok_or_else(|| "--review-rationale가 필요함".to_string())?,
        )?;
    }
    let eviction_fields = [
        args.evict_receipt.is_some(),
        args.confirm_receipt_id.is_some(),
        args.eviction_dir.is_some(),
        args.eviction_approval_dir.is_some(),
        args.journal_path.is_some(),
    ];
    if eviction_fields.iter().any(|value| *value)
        && (!eviction_fields.iter().all(|value| *value) || !attributed)
    {
        return Err(
            "eviction action에는 receipt, 확인 id, eviction dir, approval dir, journal path, reviewer, rationale가 모두 필요함".into(),
        );
    }
    let eviction_action = eviction_fields.iter().all(|value| *value) && attributed;
    if attributed && !review_action && !copy_action && !adoption_action && !eviction_action {
        return Err(
            "reviewer와 rationale는 review, copy, adoption 또는 eviction action에만 지정할 수 있음"
                .into(),
        );
    }
    let attestation_action = args.attest_receipt.is_some();
    let audit_evidence_override = args.audit_receipts && args.evidence_dir.is_some();
    if (attestation_action
        || eviction_action
        || receipt_reconcile_action
        || audit_evidence_override)
        != args.evidence_dir.is_some()
    {
        return Err("attestation/eviction action에는 --evidence-dir이 반드시 필요함".into());
    }
    if args.provider_object_id.is_some() && args.oauth_connections.is_none() {
        return Err("--provider-object-id에는 --oauth-connections가 필요함".into());
    }
    let remote_provider_api = args.oauth_connections.is_some();
    if remote_provider_api
        && args.attest_receipt.is_none()
        && !receipt_reconcile_action
        && !eviction_action
        && !copy_action
        && !args.verify_capacity
    {
        return Err(
            "provider API는 capacity, copy, attestation 또는 eviction action에만 지정할 수 있음"
                .into(),
        );
    }
    if args.verify_capacity
        && (args.list_roots
            || args.inspect_roots
            || adoption_action
            || attestation_action
            || eviction_action
            || receipt_action
            || exact_duplicate_review
            || args.export_naruon_lineage.is_some())
    {
        return Err(
            "capacity verification은 plan, review 또는 copy action에서만 사용할 수 있음".into(),
        );
    }
    if args
        .provider_object_id
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err("--provider-object-id는 비어 있을 수 없음".into());
    }
    if review_action && args.review_dir.is_none() {
        return Err("review action에는 --review-dir이 필요함".into());
    }
    if args.review_dir.is_some() && !review_action && !copy_action && !adoption_action {
        return Err("--review-dir은 review, copy, adoption action에만 지정할 수 있음".into());
    }
    if args.naruon_sync_evidence.is_some() && args.export_naruon_lineage.is_none() {
        return Err("--naruon-sync-evidence에는 --export-naruon-lineage가 필요함".into());
    }
    if args.export_naruon_capacity && !args.verify_capacity {
        return Err("--export-naruon-capacity에는 --verify-capacity가 필요함".into());
    }
    if args.export_naruon_copy_readiness && !args.verify_capacity {
        return Err("--export-naruon-copy-readiness에는 --verify-capacity가 필요함".into());
    }
    if args.naruon_copy_readiness_output.is_some() && !args.export_naruon_copy_readiness {
        return Err(
            "--naruon-copy-readiness-output에는 --export-naruon-copy-readiness가 필요함".into(),
        );
    }
    if args
        .naruon_copy_readiness_output
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err("--naruon-copy-readiness-output은 절대 경로여야 함".into());
    }
    let actions = usize::from(args.list_roots)
        + usize::from(args.inspect_roots)
        + usize::from(receipt_action)
        + usize::from(copy_action)
        + usize::from(adoption_action)
        + usize::from(args.attest_receipt.is_some())
        + usize::from(eviction_action)
        + usize::from(review_action)
        + usize::from(args.export_naruon_lineage.is_some())
        + usize::from(args.export_naruon_capacity)
        + usize::from(args.export_naruon_copy_readiness)
        + usize::from(args.export_semantic_catalog);
    if args.all_readable_roots && actions > 0 {
        return Err(
            "--all-readable-roots는 mutation 또는 root inspection과 함께 사용할 수 없음".into(),
        );
    }
    if exact_duplicate_review
        && (actions > 0
            || args.all_readable_roots
            || args.decision_summary
            || args.review_reason_set.is_some())
    {
        return Err(
            "exact duplicate review batch는 다른 action 또는 summary mode와 함께 사용할 수 없음"
                .into(),
        );
    }
    if args.decision_summary && actions > 0 {
        return Err("--decision-summary는 plan 출력에만 사용할 수 있음".into());
    }
    if args.review_reason_set.is_some() && !args.decision_summary {
        return Err("--review-reason-set에는 --decision-summary가 필요함".into());
    }
    if args.private_review_output.is_some()
        && (!args.decision_summary || args.review_reason_set.is_none())
    {
        return Err(
            "--private-review-output에는 --decision-summary와 --review-reason-set이 필요함".into(),
        );
    }
    if args.private_review_output.is_some() && args.all_readable_roots {
        return Err("--private-review-output은 단일 cloud destination에서만 사용할 수 있음".into());
    }
    if args.private_candidate_inspection_output.is_some() && !args.decision_summary {
        return Err("--private-candidate-inspection-output에는 --decision-summary가 필요함".into());
    }
    if args.private_candidate_inspection_output.is_some()
        && (args.review_reason_set.is_some() || args.private_review_output.is_some())
    {
        return Err(
            "--private-candidate-inspection-output은 exact review subset 출력과 함께 사용할 수 없음"
                .into(),
        );
    }
    if args.private_candidate_inspection_output.is_some() && args.all_readable_roots {
        return Err(
            "--private-candidate-inspection-output은 단일 cloud destination에서만 사용할 수 있음"
                .into(),
        );
    }
    if actions > 1 {
        return Err(
            "root inspection, receipt reconciliation, copy, adoption, attestation, eviction, review action은 동시에 사용할 수 없음".into(),
        );
    }
    for (flag, fingerprint) in [
        ("--copy-fingerprint", args.copy_fingerprint.as_ref()),
        (
            "--provider-api-copy-fingerprint",
            args.provider_api_copy_fingerprint.as_ref(),
        ),
        (
            "--adopt-existing-fingerprint",
            args.adopt_existing_fingerprint.as_ref(),
        ),
        (
            "--review-candidate-fingerprint",
            args.review_candidate_fingerprint.as_ref(),
        ),
        ("--review-fingerprint", args.review_fingerprint.as_ref()),
        ("--confirm-receipt-id", args.confirm_receipt_id.as_ref()),
    ] {
        let Some(fingerprint) = fingerprint else {
            continue;
        };
        if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("{flag}는 64자리 16진수여야 함"));
        }
    }
    if let Some(receipt_dir) = &args.receipt_dir {
        if !receipt_dir.is_absolute() {
            return Err("--receipt-dir은 절대 경로여야 함".into());
        }
    }
    if let Some(receipt_path) = &args.attest_receipt {
        if !receipt_path.is_absolute() {
            return Err("--attest-receipt는 절대 경로여야 함".into());
        }
    }
    if let Some(evidence_dir) = &args.evidence_dir {
        if !evidence_dir.is_absolute() {
            return Err("--evidence-dir은 절대 경로여야 함".into());
        }
    }
    if let Some(connection_path) = &args.oauth_connections {
        if !connection_path.is_absolute() {
            return Err("--oauth-connections는 절대 경로여야 함".into());
        }
    }
    if let Some(output_path) = &args.private_review_output {
        if !output_path.is_absolute() {
            return Err("--private-review-output은 절대 경로여야 함".into());
        }
    }
    if let Some(output_path) = &args.private_candidate_inspection_output {
        if !output_path.is_absolute() {
            return Err("--private-candidate-inspection-output은 절대 경로여야 함".into());
        }
    }
    if let Some(receipt_path) = &args.evict_receipt {
        if !receipt_path.is_absolute() {
            return Err("--evict-receipt는 절대 경로여야 함".into());
        }
    }
    if let Some(eviction_dir) = &args.eviction_dir {
        if !eviction_dir.is_absolute() {
            return Err("--eviction-dir은 절대 경로여야 함".into());
        }
    }
    if let Some(approval_dir) = &args.eviction_approval_dir {
        if !approval_dir.is_absolute() {
            return Err("--eviction-approval-dir은 절대 경로여야 함".into());
        }
    }
    if let Some(journal_path) = &args.journal_path {
        if !journal_path.is_absolute() {
            return Err("--journal-path는 절대 경로여야 함".into());
        }
    }
    if let Some(review_dir) = &args.review_dir {
        if !review_dir.is_absolute() {
            return Err("--review-dir은 절대 경로여야 함".into());
        }
    }
    for (flag, path) in [
        ("--export-naruon-lineage", &args.export_naruon_lineage),
        ("--naruon-sync-evidence", &args.naruon_sync_evidence),
    ] {
        if path.as_ref().is_some_and(|path| !path.is_absolute()) {
            return Err(format!("{flag}는 절대 경로여야 함"));
        }
    }
    Ok(())
}

#[cfg(not(coverage))]
fn candidate_decision_state(candidate: &cloud::CloudCandidate) -> &'static str {
    if candidate.blocked_reason.is_some() {
        "blocked"
    } else if candidate.requires_review {
        "review-required"
    } else {
        "ready-for-copy-review"
    }
}

#[cfg(not(coverage))]
fn increment(map: &mut BTreeMap<String, u64>, key: &str, value: u64) {
    let entry = map.entry(key.to_string()).or_default();
    *entry = entry.saturating_add(value);
}

/// Aggregate only fixed decision labels and evidence-source labels, never paths or metadata values.
/// Review-reason bytes can overlap because one candidate may carry several independent reasons.
#[cfg(not(coverage))]
fn decision_aggregates(report: &cloud::CloudPlanReport) -> serde_json::Value {
    let mut decision_state_counts = BTreeMap::new();
    let mut decision_state_candidate_bytes = BTreeMap::new();
    let mut review_required_reason_counts = BTreeMap::new();
    let mut review_required_reason_candidate_bytes = BTreeMap::new();
    let mut review_required_sole_reason_counts = BTreeMap::new();
    let mut review_required_sole_reason_candidate_bytes = BTreeMap::new();
    let mut review_required_reason_count_distribution = BTreeMap::new();
    let mut review_required_reason_count_candidate_bytes = BTreeMap::new();
    let mut review_required_reason_set_counts = BTreeMap::new();
    let mut review_required_reason_set_candidate_bytes = BTreeMap::new();
    let mut blocked_reason_counts = BTreeMap::new();
    let mut blocked_reason_candidate_bytes = BTreeMap::new();
    let mut production_time_source_counts = BTreeMap::new();
    let mut production_time_source_candidate_bytes = BTreeMap::new();
    let mut production_time_confidence_counts = BTreeMap::new();
    let mut production_time_confidence_candidate_bytes = BTreeMap::new();

    for candidate in &report.candidates {
        let state = candidate_decision_state(candidate);
        increment(&mut decision_state_counts, state, 1);
        increment(&mut decision_state_candidate_bytes, state, candidate.bytes);
        increment(
            &mut production_time_source_counts,
            &candidate.production_time_source,
            1,
        );
        increment(
            &mut production_time_source_candidate_bytes,
            &candidate.production_time_source,
            candidate.bytes,
        );
        increment(
            &mut production_time_confidence_counts,
            &candidate.production_time_confidence,
            1,
        );
        increment(
            &mut production_time_confidence_candidate_bytes,
            &candidate.production_time_confidence,
            candidate.bytes,
        );

        if state == "review-required" {
            let reason_count = candidate.review_reasons.len().to_string();
            let reason_set = candidate.review_reasons.join("|");
            increment(
                &mut review_required_reason_count_distribution,
                &reason_count,
                1,
            );
            increment(
                &mut review_required_reason_count_candidate_bytes,
                &reason_count,
                candidate.bytes,
            );
            increment(&mut review_required_reason_set_counts, &reason_set, 1);
            increment(
                &mut review_required_reason_set_candidate_bytes,
                &reason_set,
                candidate.bytes,
            );
            for reason in &candidate.review_reasons {
                increment(&mut review_required_reason_counts, reason, 1);
                increment(
                    &mut review_required_reason_candidate_bytes,
                    reason,
                    candidate.bytes,
                );
            }
            if let [sole_reason] = candidate.review_reasons.as_slice() {
                increment(&mut review_required_sole_reason_counts, sole_reason, 1);
                increment(
                    &mut review_required_sole_reason_candidate_bytes,
                    sole_reason,
                    candidate.bytes,
                );
            }
        }
        if state == "blocked" {
            if let Some(reason) = &candidate.blocked_reason {
                increment(&mut blocked_reason_counts, reason, 1);
                increment(&mut blocked_reason_candidate_bytes, reason, candidate.bytes);
            }
        }
    }

    serde_json::json!({
        "decision_state": {
            "counts": decision_state_counts,
            "candidate_bytes": decision_state_candidate_bytes,
        },
        "review_required_reason": {
            "counts": review_required_reason_counts,
            "candidate_bytes": review_required_reason_candidate_bytes,
            "sole_reason_counts": review_required_sole_reason_counts,
            "sole_reason_candidate_bytes": review_required_sole_reason_candidate_bytes,
            "reason_count_distribution": review_required_reason_count_distribution,
            "reason_count_candidate_bytes": review_required_reason_count_candidate_bytes,
            "reason_set_counts": review_required_reason_set_counts,
            "reason_set_candidate_bytes": review_required_reason_set_candidate_bytes,
            "reason_set_delimiter": "|",
            "candidate_bytes_can_overlap_across_reasons": true,
        },
        "blocked_reason": {
            "counts": blocked_reason_counts,
            "candidate_bytes": blocked_reason_candidate_bytes,
        },
        "production_time_source": {
            "counts": production_time_source_counts,
            "candidate_bytes": production_time_source_candidate_bytes,
        },
        "production_time_confidence": {
            "counts": production_time_confidence_counts,
            "candidate_bytes": production_time_confidence_candidate_bytes,
        },
    })
}

#[cfg(not(coverage))]
fn redacted_decision(candidate: &cloud::CloudCandidate) -> serde_json::Value {
    let approval_action = match candidate.blocked_reason.as_deref() {
        None => Some(cloud_transfer::CloudCopyApprovalAction::CopyOnly),
        Some("destination-exists") => {
            Some(cloud_transfer::CloudCopyApprovalAction::AdoptExistingCopy)
        }
        Some(_) => None,
    };
    serde_json::json!({
        "metadata_fingerprint": &candidate.metadata_fingerprint,
        "review_fingerprint": &candidate.review_fingerprint,
        "provider": candidate.provider,
        "destination_account_scope": candidate.destination_account_scope,
        "kind": candidate.kind,
        "bytes": candidate.bytes,
        "age_days": candidate.age_days,
        "production_time_ms": candidate.production_time_ms,
        "production_time_source": &candidate.production_time_source,
        "production_time_confidence": &candidate.production_time_confidence,
        "decision_state": candidate_decision_state(candidate),
        "requires_review": candidate.requires_review,
        "review_reasons": &candidate.review_reasons,
        "blocked_reason": &candidate.blocked_reason,
        "copy_approval_action": approval_action,
        "exact_copy_approval_phrase": approval_action
            .map(|action| cloud_transfer::cloud_copy_approval_phrase(candidate, action)),
        "copy_approval_max_age_ms": cloud_transfer::MAX_CLOUD_COPY_APPROVAL_AGE_MS,
    })
}

#[cfg(not(coverage))]
const REVIEW_BATCH_FINGERPRINT_VERSION: u32 = 2;
#[cfg(not(coverage))]
const PRIVATE_REVIEW_DOSSIER_MAX_BYTES: usize = 16 * 1024 * 1024;

#[cfg(not(coverage))]
fn review_batch_fingerprint(
    report: &cloud::CloudPlanReport,
    reasons: &[String],
    candidates: &[&cloud::CloudCandidate],
) -> String {
    let mut ordered = candidates.to_vec();
    ordered.sort_by(|left, right| {
        left.metadata_fingerprint
            .cmp(&right.metadata_fingerprint)
            .then_with(|| left.review_fingerprint.cmp(&right.review_fingerprint))
    });
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-cloud-review-batch-v2\0");
    hasher.update(&REVIEW_BATCH_FINGERPRINT_VERSION.to_le_bytes());
    for value in [
        report.cloud_root.provider.as_str().as_bytes(),
        report.cloud_root.account_scope.as_str().as_bytes(),
    ] {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value);
    }
    for reason in reasons {
        hasher.update(reason.as_bytes());
        hasher.update(&[0]);
    }
    hasher.update(&(ordered.len() as u64).to_le_bytes());
    for candidate in ordered {
        hasher.update(candidate.metadata_fingerprint.as_bytes());
        hasher.update(candidate.review_fingerprint.as_bytes());
        hasher.update(&candidate.bytes.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

#[cfg(not(coverage))]
fn exact_review_candidates<'a>(
    report: &'a cloud::CloudPlanReport,
    reasons: &[String],
) -> Result<Vec<&'a cloud::CloudCandidate>, String> {
    let candidates = report
        .candidates
        .iter()
        .filter(|candidate| {
            candidate_decision_state(candidate) == "review-required"
                && candidate.review_reasons.as_slice() == reasons
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err("현재 fresh plan에 exact review reason set이 일치하는 후보가 없음".into());
    }
    Ok(candidates)
}

/// Produce an exact reason-set slice for inspection. The stable subset fingerprint binds only the
/// selected evidence; the separate decision-batch fingerprint records full-plan freshness. Neither
/// is approval: every approve/hold decision remains individually attributed and candidate-bound.
#[cfg(not(coverage))]
fn review_batch_summary(
    report: &cloud::CloudPlanReport,
    reasons: &[String],
) -> Result<serde_json::Value, String> {
    let candidates = exact_review_candidates(report, reasons)?;
    let candidate_bytes = candidates.iter().fold(0u64, |total, candidate| {
        total.saturating_add(candidate.bytes)
    });
    let batch_fingerprint = review_batch_fingerprint(report, reasons, &candidates);
    let decisions = candidates
        .into_iter()
        .map(redacted_decision)
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "schema_version": 3,
        "output_mode": "review-batch-summary",
        "generated_at_ms": report.generated_at_ms,
        "source_selection_policy": report.source_selection_policy,
        "decision_batch_fingerprint_version": cloud::CLOUD_DECISION_BATCH_FINGERPRINT_VERSION,
        "decision_batch_fingerprint": cloud::cloud_decision_batch_fingerprint(report),
        "review_batch_fingerprint_version": REVIEW_BATCH_FINGERPRINT_VERSION,
        "review_batch_fingerprint": batch_fingerprint,
        "reason_set": reasons,
        "cloud": {
            "provider": report.cloud_root.provider,
            "account_scope": report.cloud_root.account_scope,
        },
        "candidate_count": decisions.len(),
        "candidate_bytes": candidate_bytes,
        "metadata_policy": {
            "production_time_precedence": [
                "embedded-metadata",
                "explicit-filename-date",
                "filesystem-created",
                "filesystem-modified",
            ],
            "filename_dates_are_auxiliary": true,
            "summary_is_dry_run_only": true,
            "batch_fingerprint_is_not_approval": true,
            "candidate_review_decisions_remain_individual": true,
            "exact_human_attributed_copy_approval_required": true,
            "copy_approval_max_age_ms": cloud_transfer::MAX_CLOUD_COPY_APPROVAL_AGE_MS,
        },
        "redacted_from_summary": [
            "absolute-source-path",
            "absolute-destination-path",
            "relative-source-path-and-file-name",
            "cloud-root-path-and-label",
            "content-title-and-authors",
            "raw-metadata-evidence-values",
            "dataset-profile",
        ],
        "decisions": decisions,
    }))
}

/// Build the private, full-evidence counterpart of an exact reason-set review summary.
///
/// Unlike `review_batch_summary`, this value intentionally contains sensitive local paths and raw
/// embedded metadata. It must only be written through `write_private_review_dossier`.
#[cfg(not(coverage))]
fn private_review_dossier(
    report: &cloud::CloudPlanReport,
    reasons: &[String],
) -> Result<serde_json::Value, String> {
    let mut candidates = exact_review_candidates(report, reasons)?;
    candidates.sort_by(|left, right| {
        left.metadata_fingerprint
            .cmp(&right.metadata_fingerprint)
            .then_with(|| left.review_fingerprint.cmp(&right.review_fingerprint))
    });
    let candidate_bytes = candidates.iter().fold(0u64, |total, candidate| {
        total.saturating_add(candidate.bytes)
    });
    let review_batch_fingerprint = review_batch_fingerprint(report, reasons, &candidates);

    Ok(serde_json::json!({
        "schema_version": 1,
        "output_mode": "private-review-dossier",
        "generated_at_ms": report.generated_at_ms,
        "contains_sensitive_local_metadata": true,
        "source_selection_policy": report.source_selection_policy,
        "decision_batch_fingerprint_version": cloud::CLOUD_DECISION_BATCH_FINGERPRINT_VERSION,
        "decision_batch_fingerprint": cloud::cloud_decision_batch_fingerprint(report),
        "review_batch_fingerprint_version": REVIEW_BATCH_FINGERPRINT_VERSION,
        "review_batch_fingerprint": review_batch_fingerprint,
        "reason_set": reasons,
        "cloud_root": &report.cloud_root,
        "candidate_count": candidates.len(),
        "candidate_bytes": candidate_bytes,
        "metadata_policy": {
            "production_time_precedence": [
                "embedded-metadata",
                "explicit-filename-date",
                "filesystem-created",
                "filesystem-modified",
            ],
            "filename_dates_are_auxiliary": true,
            "raw_metadata_values_are_review_evidence_only": true,
            "dossier_is_not_approval": true,
            "candidate_review_decisions_remain_individual": true,
        },
        "candidates": candidates,
    }))
}

#[cfg(not(coverage))]
#[derive(serde::Serialize)]
struct PrivateCandidateInspection<'a> {
    decision_state: &'static str,
    #[serde(flatten)]
    candidate: &'a cloud::CloudCandidate,
}

/// Build a private, full-evidence inspection dossier for every candidate in one fresh plan.
///
/// This intentionally includes blocked candidates because their paths and embedded metadata still
/// need human inspection even though they cannot enter the review or copy gates. The dossier is
/// evidence only: it does not create review decisions, authorize a copy, or authorize eviction.
#[cfg(not(coverage))]
fn private_candidate_inspection_dossier(report: &cloud::CloudPlanReport) -> serde_json::Value {
    let mut candidates = report.candidates.iter().collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.metadata_fingerprint
            .cmp(&right.metadata_fingerprint)
            .then_with(|| left.review_fingerprint.cmp(&right.review_fingerprint))
    });
    let candidate_bytes = candidates.iter().fold(0u64, |total, candidate| {
        total.saturating_add(candidate.bytes)
    });
    let mut decision_state_counts = BTreeMap::<&'static str, u64>::new();
    let mut decision_state_bytes = BTreeMap::<&'static str, u64>::new();
    let inspections = candidates
        .into_iter()
        .map(|candidate| {
            let decision_state = candidate_decision_state(candidate);
            *decision_state_counts.entry(decision_state).or_insert(0) += 1;
            let bytes = decision_state_bytes.entry(decision_state).or_insert(0);
            *bytes = bytes.saturating_add(candidate.bytes);
            PrivateCandidateInspection {
                decision_state,
                candidate,
            }
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "schema_version": 1,
        "output_mode": "private-candidate-inspection-dossier",
        "generated_at_ms": report.generated_at_ms,
        "contains_sensitive_local_metadata": true,
        "inspection_scope": "all-current-plan-candidates",
        "source_selection_policy": report.source_selection_policy,
        "decision_batch_fingerprint_version": cloud::CLOUD_DECISION_BATCH_FINGERPRINT_VERSION,
        "decision_batch_fingerprint": cloud::cloud_decision_batch_fingerprint(report),
        "cloud_root": &report.cloud_root,
        "candidate_count": inspections.len(),
        "candidate_bytes": candidate_bytes,
        "decision_state": {
            "counts": decision_state_counts,
            "candidate_bytes": decision_state_bytes,
        },
        "metadata_policy": {
            "production_time_precedence": [
                "embedded-metadata",
                "explicit-filename-date",
                "filesystem-created",
                "filesystem-modified",
            ],
            "filename_dates_are_auxiliary": true,
            "raw_metadata_values_are_review_evidence_only": true,
            "inspection_includes_blocked_candidates": true,
            "dossier_is_not_approval": true,
            "candidate_review_decisions_remain_individual": true,
        },
        "cloud_write_executed": false,
        "source_eviction_authorized": false,
        "candidates": inspections,
    })
}

#[cfg(all(not(coverage), unix))]
fn write_private_review_dossier(
    path: &Path,
    dossier: &serde_json::Value,
) -> Result<(String, usize), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "private-review-output-parent-missing".to_string())?;
    let parent_metadata = std::fs::symlink_metadata(parent)
        .map_err(|_| "private-review-output-parent-unavailable".to_string())?;
    if !parent_metadata.is_dir() || parent_metadata.file_type().is_symlink() {
        return Err("private-review-output-parent-unsafe".into());
    }

    let encoded = serde_json::to_vec_pretty(dossier)
        .map_err(|_| "private-review-output-json-invalid".to_string())?;
    if encoded.len() > PRIVATE_REVIEW_DOSSIER_MAX_BYTES {
        return Err("private-review-output-too-large".into());
    }

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "private-review-output-create-failed".to_string())?;
    let result = (|| -> Result<(), String> {
        file.write_all(&encoded)
            .and_then(|_| file.sync_all())
            .map_err(|_| "private-review-output-write-failed".to_string())?;
        let metadata = file
            .metadata()
            .map_err(|_| "private-review-output-metadata-failed".to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("private-review-output-unsafe".into());
        }
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o777 != 0o600 {
                return Err("private-review-output-mode-invalid".into());
            }
            std::fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| "private-review-output-parent-sync-failed".to_string())?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(error);
    }

    let sha256 = Sha256::digest(&encoded)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok((sha256, encoded.len()))
}

#[cfg(all(not(coverage), not(unix)))]
fn write_private_review_dossier(
    _path: &Path,
    _dossier: &serde_json::Value,
) -> Result<(String, usize), String> {
    Err("private-review-output-secure-mode-unsupported".into())
}

#[cfg(not(coverage))]
const EXACT_DUPLICATE_REVIEW_BATCH_FINGERPRINT_VERSION: u32 = 1;

#[cfg(not(coverage))]
#[derive(Debug, Clone, serde::Serialize)]
struct RedactedMetadataEvidence {
    field: String,
    source: String,
    confidence: String,
}

#[cfg(not(coverage))]
#[derive(Debug, Clone, serde::Serialize)]
struct ExactDuplicateReviewMember {
    metadata_fingerprint: String,
    review_fingerprint: String,
    relative_path: String,
    kind: ArchiveKind,
    bytes: u64,
    production_time_ms: u64,
    production_time_source: String,
    production_time_confidence: String,
    production_evidence: Vec<RedactedMetadataEvidence>,
    source_lineage_evidence_fields: Vec<String>,
    canonical_recommendation: &'static str,
    review_reasons: Vec<String>,
}

#[cfg(not(coverage))]
#[derive(Debug, Clone, serde::Serialize)]
struct ExactDuplicateReviewCluster {
    cluster_fingerprint: String,
    content_sha256: String,
    bytes_per_candidate: u64,
    candidate_count: usize,
    redundant_bytes: u64,
    recommendation_confidence: String,
    recommendation_reason_codes: Vec<String>,
    canonical: ExactDuplicateReviewMember,
    redundant_copies: Vec<ExactDuplicateReviewMember>,
    requires_human_confirmation: bool,
}

#[cfg(not(coverage))]
fn archive_kind_label(kind: ArchiveKind) -> &'static str {
    match kind {
        ArchiveKind::Document => "document",
        ArchiveKind::Media => "media",
        ArchiveKind::Archive => "archive",
        ArchiveKind::Dataset => "dataset",
        ArchiveKind::Backup => "backup",
        ArchiveKind::Creative => "creative",
        ArchiveKind::IncompleteDownload => "incomplete-download",
        ArchiveKind::SensitiveConfig => "sensitive-config",
    }
}

#[cfg(not(coverage))]
fn normal_relative_components(path: &str) -> Option<Vec<String>> {
    let mut values = Vec::new();
    for component in Path::new(path).components() {
        let std::path::Component::Normal(value) = component else {
            return None;
        };
        values.push(value.to_str()?.to_string());
    }
    (!values.is_empty()).then_some(values)
}

#[cfg(not(coverage))]
fn exact_duplicate_content_sha256(candidate: &cloud::CloudCandidate) -> Result<String, String> {
    let mut values = candidate
        .metadata_evidence
        .iter()
        .filter(|evidence| evidence.field == "exact-duplicate-content-sha256")
        .map(|evidence| evidence.value.clone())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    match values.as_slice() {
        [value]
            if value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) =>
        {
            Ok(value.clone())
        }
        [] => Err("exact-duplicate-content-sha256-missing".into()),
        _ => Err("exact-duplicate-content-sha256-invalid-or-conflicting".into()),
    }
}

#[cfg(not(coverage))]
fn exact_duplicate_review_member(
    candidate: &cloud::CloudCandidate,
    canonical: bool,
) -> ExactDuplicateReviewMember {
    let mut production_evidence = candidate
        .metadata_evidence
        .iter()
        .filter(|evidence| {
            matches!(
                evidence.field.as_str(),
                "production-date"
                    | "filename-date-hint"
                    | "filesystem-created-date"
                    | "filesystem-modified-date"
            )
        })
        .map(|evidence| RedactedMetadataEvidence {
            field: evidence.field.clone(),
            source: evidence.source.clone(),
            confidence: evidence.confidence.clone(),
        })
        .collect::<Vec<_>>();
    production_evidence.sort_by(|left, right| {
        left.field
            .cmp(&right.field)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.confidence.cmp(&right.confidence))
    });
    production_evidence.dedup_by(|left, right| {
        left.field == right.field
            && left.source == right.source
            && left.confidence == right.confidence
    });

    let mut source_lineage_evidence_fields = candidate
        .content_context
        .iter()
        .filter_map(|context| context.split_once('=').map(|(field, _)| field.to_string()))
        .collect::<Vec<_>>();
    source_lineage_evidence_fields.sort();
    source_lineage_evidence_fields.dedup();

    ExactDuplicateReviewMember {
        metadata_fingerprint: candidate.metadata_fingerprint.clone(),
        review_fingerprint: candidate.review_fingerprint.clone(),
        relative_path: candidate.relative_path.clone(),
        kind: candidate.kind,
        bytes: candidate.bytes,
        production_time_ms: candidate.production_time_ms,
        production_time_source: candidate.production_time_source.clone(),
        production_time_confidence: candidate.production_time_confidence.clone(),
        production_evidence,
        source_lineage_evidence_fields,
        canonical_recommendation: if canonical {
            "preferred"
        } else {
            "redundant-copy-candidate"
        },
        review_reasons: candidate.review_reasons.clone(),
    }
}

#[cfg(not(coverage))]
fn hash_exact_duplicate_review_value(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

#[cfg(not(coverage))]
fn exact_duplicate_review_batch_fingerprint(
    prefix: &str,
    kind: ArchiveKind,
    clusters: &[ExactDuplicateReviewCluster],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-exact-duplicate-review-batch-v1\0");
    hasher.update(&EXACT_DUPLICATE_REVIEW_BATCH_FINGERPRINT_VERSION.to_le_bytes());
    hash_exact_duplicate_review_value(&mut hasher, prefix.as_bytes());
    hash_exact_duplicate_review_value(&mut hasher, archive_kind_label(kind).as_bytes());
    hash_exact_duplicate_review_value(&mut hasher, &(clusters.len() as u64).to_le_bytes());
    for cluster in clusters {
        for value in [
            cluster.cluster_fingerprint.as_bytes(),
            cluster.content_sha256.as_bytes(),
            cluster.canonical.metadata_fingerprint.as_bytes(),
            cluster.canonical.review_fingerprint.as_bytes(),
            cluster.canonical.relative_path.as_bytes(),
        ] {
            hash_exact_duplicate_review_value(&mut hasher, value);
        }
        hash_exact_duplicate_review_value(&mut hasher, &cluster.bytes_per_candidate.to_le_bytes());
        hash_exact_duplicate_review_value(&mut hasher, &cluster.redundant_bytes.to_le_bytes());
        for member in &cluster.redundant_copies {
            for value in [
                member.metadata_fingerprint.as_bytes(),
                member.review_fingerprint.as_bytes(),
                member.relative_path.as_bytes(),
            ] {
                hash_exact_duplicate_review_value(&mut hasher, value);
            }
            hash_exact_duplicate_review_value(&mut hasher, &member.bytes.to_le_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

/// Emit a conservative, source-local review slice for exact duplicates.
///
/// The selected canonical copy must be a direct child of the source root. Every redundant member
/// must be nested beneath a first path component that begins with the operator-supplied prefix.
/// The output is evidence for a later human decision; it cannot authorize or execute Trash.
#[cfg(not(coverage))]
fn exact_duplicate_review_batch(
    report: &cloud::CloudPlanReport,
    redundant_prefix: &str,
    kind: ArchiveKind,
) -> Result<serde_json::Value, String> {
    let mut selected = Vec::new();
    for cluster in &report.exact_duplicates.clusters {
        if cluster.recommendation_confidence != "high"
            || cluster.recommendation_reason_codes.as_slice()
                != ["richer-source-lineage-context-preferred"]
        {
            continue;
        }
        if !cluster.requires_human_confirmation
            || cluster.candidate_count < 2
            || cluster.member_metadata_fingerprints.len() != cluster.candidate_count
            || cluster.redundant_bytes
                != cluster
                    .bytes_per_candidate
                    .saturating_mul((cluster.candidate_count - 1) as u64)
        {
            return Err("exact-duplicate-review-cluster-contract-invalid".into());
        }

        let mut members = Vec::with_capacity(cluster.member_metadata_fingerprints.len());
        for fingerprint in &cluster.member_metadata_fingerprints {
            let matches = report
                .candidates
                .iter()
                .filter(|candidate| candidate.metadata_fingerprint == *fingerprint)
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [only] => members.push(*only),
                [] => {
                    return Err(format!(
                        "exact-duplicate-review-candidate-missing-from-bounded-plan:{fingerprint}"
                    ));
                }
                _ => {
                    return Err(format!(
                        "exact-duplicate-review-candidate-fingerprint-ambiguous:{fingerprint}"
                    ));
                }
            }
        }
        if members
            .iter()
            .any(|candidate| candidate.bytes != cluster.bytes_per_candidate)
        {
            return Err("exact-duplicate-review-member-size-mismatch".into());
        }
        let canonical = members
            .iter()
            .copied()
            .filter(|candidate| {
                candidate.metadata_fingerprint == cluster.recommended_canonical_metadata_fingerprint
            })
            .collect::<Vec<_>>();
        let canonical = match canonical.as_slice() {
            [only] => *only,
            _ => return Err("exact-duplicate-review-canonical-not-unique".into()),
        };
        if canonical.kind != kind
            || normal_relative_components(&canonical.relative_path)
                .is_none_or(|components| components.len() != 1)
        {
            continue;
        }

        let mut redundant = members
            .into_iter()
            .filter(|candidate| candidate.metadata_fingerprint != canonical.metadata_fingerprint)
            .collect::<Vec<_>>();
        if redundant.is_empty() || redundant.iter().any(|candidate| candidate.kind != kind) {
            continue;
        }
        let all_under_prefix = redundant.iter().all(|candidate| {
            normal_relative_components(&candidate.relative_path).is_some_and(|components| {
                components.len() > 1 && components[0].starts_with(redundant_prefix)
            })
        });
        if !all_under_prefix {
            continue;
        }
        redundant.sort_by(|left, right| {
            left.relative_path
                .cmp(&right.relative_path)
                .then_with(|| left.metadata_fingerprint.cmp(&right.metadata_fingerprint))
        });

        let content_sha256 = exact_duplicate_content_sha256(canonical)?;
        for member in &redundant {
            if exact_duplicate_content_sha256(member)? != content_sha256 {
                return Err("exact-duplicate-review-content-sha256-mismatch".into());
            }
        }
        selected.push(ExactDuplicateReviewCluster {
            cluster_fingerprint: cluster.cluster_fingerprint.clone(),
            content_sha256,
            bytes_per_candidate: cluster.bytes_per_candidate,
            candidate_count: cluster.candidate_count,
            redundant_bytes: cluster.redundant_bytes,
            recommendation_confidence: cluster.recommendation_confidence.clone(),
            recommendation_reason_codes: cluster.recommendation_reason_codes.clone(),
            canonical: exact_duplicate_review_member(canonical, true),
            redundant_copies: redundant
                .into_iter()
                .map(|candidate| exact_duplicate_review_member(candidate, false))
                .collect(),
            requires_human_confirmation: true,
        });
    }
    if selected.is_empty() {
        return Err("현재 fresh plan에 제한 조건을 만족하는 exact duplicate가 없음".into());
    }
    selected.sort_by(|left, right| left.cluster_fingerprint.cmp(&right.cluster_fingerprint));
    let batch_fingerprint =
        exact_duplicate_review_batch_fingerprint(redundant_prefix, kind, &selected);
    let redundant_copy_count = selected
        .iter()
        .map(|cluster| cluster.redundant_copies.len())
        .sum::<usize>();
    let redundant_bytes = selected.iter().fold(0u64, |total, cluster| {
        total.saturating_add(cluster.redundant_bytes)
    });

    Ok(serde_json::json!({
        "schema_version": 1,
        "output_mode": "exact-duplicate-review-batch",
        "generated_at_ms": report.generated_at_ms,
        "exact_duplicate_review_batch_fingerprint_version":
            EXACT_DUPLICATE_REVIEW_BATCH_FINGERPRINT_VERSION,
        "exact_duplicate_review_batch_fingerprint": batch_fingerprint,
        "selection": {
            "recommendation_confidence": "high",
            "recommendation_reason_codes_exact": [
                "richer-source-lineage-context-preferred",
            ],
            "canonical_location": "direct-child-of-source-root",
            "redundant_first_component_prefix": redundant_prefix,
            "kind": kind,
        },
        "metadata_policy": {
            "production_time_precedence": [
                "embedded-metadata",
                "explicit-filename-date",
                "filesystem-created",
                "filesystem-modified",
            ],
            "filename_dates_are_auxiliary": true,
            "summary_is_dry_run_only": true,
            "batch_fingerprint_is_not_approval": true,
            "human_confirmation_required_for_every_cluster": true,
            "trash_execution_is_not_available_in_this_output_mode": true,
        },
        "redacted_from_summary": [
            "absolute-source-path",
            "absolute-destination-path",
            "cloud-root-path-and-label",
            "content-title-and-authors",
            "raw-metadata-evidence-values",
            "source-lineage-evidence-values",
            "dataset-profile",
        ],
        "cluster_count": selected.len(),
        "candidate_count": selected.len().saturating_add(redundant_copy_count),
        "redundant_copy_count": redundant_copy_count,
        "redundant_bytes": redundant_bytes,
        "clusters": selected,
    }))
}

/// Produce a bounded operator view without paths, file names, or raw embedded metadata values.
///
/// The full plan remains the durable lineage source. This view is intentionally limited to the
/// evidence needed to select a candidate for a separately attributed human review.
#[cfg(not(coverage))]
fn decision_summary(report: &cloud::CloudPlanReport) -> serde_json::Value {
    let aggregates = decision_aggregates(report);
    let decisions = report
        .candidates
        .iter()
        .map(redacted_decision)
        .collect::<Vec<_>>();

    serde_json::json!({
        "schema_version": 3,
        "output_mode": "decision-summary",
        "generated_at_ms": report.generated_at_ms,
        "source_selection_policy": report.source_selection_policy,
        "decision_batch_fingerprint_version": cloud::CLOUD_DECISION_BATCH_FINGERPRINT_VERSION,
        "decision_batch_fingerprint": cloud::cloud_decision_batch_fingerprint(report),
        "metadata_policy": {
            "production_time_precedence": [
                "embedded-metadata",
                "explicit-filename-date",
                "filesystem-created",
                "filesystem-modified",
            ],
            "filename_dates_are_auxiliary": true,
            "summary_is_dry_run_only": true,
            "review_fingerprints_bind_operator_decisions": true,
            "exact_human_attributed_copy_approval_required": true,
            "copy_approval_is_bound_to_review_fingerprint_and_action": true,
            "verified_provider_sync_required_before_local_eviction": true,
        },
        "cloud": {
            "provider": report.cloud_root.provider,
            "account_scope": report.cloud_root.account_scope,
        },
        "candidate_count": report.candidates.len(),
        "candidate_bytes": report.candidate_bytes,
        "potentially_reclaimable_bytes": report.potentially_reclaimable_bytes,
        "aggregates": aggregates,
        "exact_duplicates": &report.exact_duplicates,
        "capacity": &report.capacity,
        "notices": &report.notices,
        "redacted_from_summary": [
            "absolute-source-path",
            "absolute-destination-path",
            "relative-source-path-and-file-name",
            "cloud-root-path-and-label",
            "content-title-and-authors",
            "raw-metadata-evidence-values",
            "dataset-profile",
        ],
        "decisions": decisions,
    })
}

/// Summarize the concrete lineage-preserving destination manifest without exposing any path or
/// embedded metadata value. Counts and bytes are deterministic for the decision batch.
#[cfg(not(coverage))]
fn organization_manifest_summary(report: &cloud::CloudPlanReport) -> serde_json::Value {
    let mut kind_counts = BTreeMap::new();
    let mut kind_bytes = BTreeMap::new();
    let mut production_month_counts = BTreeMap::new();
    let mut production_month_bytes = BTreeMap::new();
    let mut context_coverage_counts = BTreeMap::new();
    let mut context_coverage_bytes = BTreeMap::new();
    let mut destination_collision_count = 0_u64;
    let mut destination_collision_bytes = 0_u64;

    for candidate in &report.candidates {
        let kind = archive_kind_label(candidate.kind);
        increment(&mut kind_counts, kind, 1);
        increment(&mut kind_bytes, kind, candidate.bytes);

        let (year, month) = cloud::production_year_month(candidate.production_time_ms);
        let production_month = format!("{year:04}-{month:02}");
        increment(&mut production_month_counts, &production_month, 1);
        increment(
            &mut production_month_bytes,
            &production_month,
            candidate.bytes,
        );

        for (label, present) in [
            ("content-title-present", candidate.content_title.is_some()),
            (
                "content-authors-present",
                !candidate.content_authors.is_empty(),
            ),
            (
                "content-context-present",
                !candidate.content_context.is_empty(),
            ),
            (
                "nested-source-context-preserved",
                candidate.source_context != ".",
            ),
            (
                "embedded-metadata-present",
                candidate
                    .metadata_evidence
                    .iter()
                    .any(|evidence| evidence.source.starts_with("embedded:")),
            ),
        ] {
            if present {
                increment(&mut context_coverage_counts, label, 1);
                increment(&mut context_coverage_bytes, label, candidate.bytes);
            }
        }

        if candidate.blocked_reason.as_deref() == Some("destination-exists") {
            destination_collision_count = destination_collision_count.saturating_add(1);
            destination_collision_bytes =
                destination_collision_bytes.saturating_add(candidate.bytes);
        }
    }

    serde_json::json!({
        "layout_policy":
            "DiskSage Archive/{production-year}/{production-month}/{archive-kind}/{source-relative-path}",
        "production_time_drives_year_and_month": true,
        "source_relative_path_preserved_for_lineage": true,
        "bound_to_decision_batch_fingerprint": true,
        "candidate_groups": {
            "archive_kind": {
                "counts": kind_counts,
                "candidate_bytes": kind_bytes,
            },
            "production_month": {
                "counts": production_month_counts,
                "candidate_bytes": production_month_bytes,
            },
        },
        "context_evidence_coverage": {
            "counts": context_coverage_counts,
            "candidate_bytes": context_coverage_bytes,
            "candidate_bytes_can_overlap_across_evidence_kinds": true,
        },
        "destination_collision_preflight": {
            "counts": destination_collision_count,
            "candidate_bytes": destination_collision_bytes,
            "collision_policy": "block-do-not-overwrite",
        },
        "manifest_is_dry_run_only": true,
        "human_review_required_before_copy": true,
    })
}

/// Produce a small, path-free overview for comparing destinations before opening a private
/// candidate dossier. Unlike `decision_summary`, this deliberately omits per-candidate
/// fingerprints, combinatorial reason sets, and duplicate-cluster membership.
#[cfg(not(coverage))]
fn compact_decision_summary(report: &cloud::CloudPlanReport) -> serde_json::Value {
    let aggregates = decision_aggregates(report);
    let review_required = &aggregates["review_required_reason"];
    let organization_manifest = organization_manifest_summary(report);

    serde_json::json!({
        "schema_version": 3,
        "output_mode": "compact-decision-summary",
        "generated_at_ms": report.generated_at_ms,
        "source_selection_policy": report.source_selection_policy,
        "decision_batch_fingerprint_version": cloud::CLOUD_DECISION_BATCH_FINGERPRINT_VERSION,
        "decision_batch_fingerprint": cloud::cloud_decision_batch_fingerprint(report),
        "metadata_policy": {
            "production_time_precedence": [
                "embedded-metadata",
                "explicit-filename-date",
                "filesystem-created",
                "filesystem-modified",
            ],
            "filename_dates_are_auxiliary": true,
            "summary_is_dry_run_only": true,
            "batch_fingerprint_is_not_approval": true,
            "private_candidate_review_required_before_copy": true,
            "exact_human_attributed_copy_approval_required": true,
            "copy_approval_max_age_ms": cloud_transfer::MAX_CLOUD_COPY_APPROVAL_AGE_MS,
            "verified_provider_sync_required_before_local_eviction": true,
        },
        "cloud": {
            "provider": report.cloud_root.provider,
            "account_scope": report.cloud_root.account_scope,
        },
        "candidate_count": report.candidates.len(),
        "candidate_bytes": report.candidate_bytes,
        "potentially_reclaimable_bytes": report.potentially_reclaimable_bytes,
        "aggregates": {
            "decision_state": aggregates["decision_state"].clone(),
            "review_required_reason": {
                "counts": review_required["counts"].clone(),
                "candidate_bytes": review_required["candidate_bytes"].clone(),
                "sole_reason_counts": review_required["sole_reason_counts"].clone(),
                "sole_reason_candidate_bytes":
                    review_required["sole_reason_candidate_bytes"].clone(),
                "candidate_bytes_can_overlap_across_reasons": true,
            },
            "blocked_reason": aggregates["blocked_reason"].clone(),
            "production_time_source": aggregates["production_time_source"].clone(),
            "production_time_confidence":
                aggregates["production_time_confidence"].clone(),
        },
        "exact_duplicates": {
            "cluster_count": report.exact_duplicates.cluster_count,
            "candidate_count": report.exact_duplicates.candidate_count,
            "candidate_bytes": report.exact_duplicates.candidate_bytes,
            "redundant_bytes": report.exact_duplicates.redundant_bytes,
            "cluster_members_omitted": true,
            "human_confirmation_required": true,
        },
        "organization_manifest": organization_manifest,
        "capacity": &report.capacity,
        "notices": &report.notices,
        "next_step": {
            "detailed_redacted_queue": "--decision-summary",
            "private_exact_reason_dossier":
                "--decision-summary --review-reason-set REASON|REASON --private-review-output ABSOLUTE_NEW_FILE.json",
        },
        "redacted_from_summary": [
            "absolute-source-path",
            "absolute-destination-path",
            "relative-source-path-and-file-name",
            "cloud-root-path-and-label",
            "content-title-and-authors",
            "raw-metadata-evidence-values",
            "dataset-profile",
            "candidate-metadata-and-review-fingerprints",
            "review-reason-set-combinations",
            "exact-duplicate-cluster-members",
        ],
        "candidate_details_included": false,
        "cloud_write_executed": false,
        "source_eviction_authorized": false,
    })
}

#[cfg(not(coverage))]
fn receipt_cloud_root(receipt: &CloudCopyReceipt, home: &Path) -> Result<CloudRoot, String> {
    let destination = Path::new(&receipt.destination);
    cloud::discover_cloud_roots(home)
        .into_iter()
        .filter(|root| {
            root.provider == receipt.provider && destination.starts_with(Path::new(&root.path))
        })
        .max_by_key(|root| Path::new(&root.path).components().count())
        .ok_or_else(|| "receipt-cloud-root-unavailable".to_string())
}

#[cfg(not(coverage))]
fn cloud_projection_dirs(anchor: &Path) -> (PathBuf, PathBuf) {
    let parent = anchor.parent().unwrap_or(anchor);
    (parent.join("cloud-adr"), parent.join("cloud-goals"))
}

#[cfg(not(coverage))]
fn collect_root_capacity(
    root: &CloudRoot,
    oauth_connections: Option<&Path>,
    observed_at_ms: u64,
) -> Result<provider_capacity::CloudCapacitySnapshot, String> {
    if root.provider == CloudProvider::Icloud {
        return provider_capacity::collect_icloud_native_capacity(observed_at_ms);
    }
    let connection_path = oauth_connections
        .ok_or_else(|| "provider-capacity-oauth-connections-required".to_string())?;
    let access_token = provider_oauth::refreshed_access_token(connection_path, root)?;
    provider_capacity::collect_authenticated_capacity(
        root.provider,
        access_token.as_str(),
        observed_at_ms,
        &FixedHostProviderCapacityClient::default(),
    )
}

#[cfg(not(coverage))]
fn attach_capacity_snapshot(
    report: &mut cloud::CloudPlanReport,
    snapshot: provider_capacity::CloudCapacitySnapshot,
    reserve_mib: u64,
) -> Result<(), String> {
    if snapshot.provider != report.cloud_root.provider
        || snapshot.account_scope.is_some_and(|scope| {
            report.cloud_root.account_scope != CloudAccountScope::Unknown
                && report.cloud_root.account_scope != scope
        })
    {
        return Err("cloud-capacity-root-binding-mismatch".into());
    }
    let largest_candidate_bytes = report
        .candidates
        .iter()
        .filter(|candidate| candidate.blocked_reason.is_none())
        .map(|candidate| candidate.bytes)
        .max()
        .unwrap_or_default();
    let assessment = provider_capacity::assess_capacity(
        snapshot,
        report.potentially_reclaimable_bytes,
        largest_candidate_bytes,
        reserve_mib.saturating_mul(1024 * 1024),
    );
    report
        .notices
        .retain(|notice| notice != "cloud-quota-unverified");
    report.notices.push(
        match assessment.can_fit {
            Some(true)
                if assessment.snapshot.evidence_kind
                    == provider_capacity::CapacityEvidenceKind::ProviderNativeStatus =>
            {
                "cloud-quota-provider-native-verified"
            }
            Some(true) => "cloud-quota-provider-api-verified",
            Some(false) => "cloud-quota-insufficient-or-blocked",
            None => "cloud-quota-unavailable",
        }
        .into(),
    );
    report.capacity = Some(assessment);
    Ok(())
}

#[cfg(not(coverage))]
fn plan_with_optional_capacity(
    source: &cloud::CloudSourceSnapshot,
    root: &CloudRoot,
    verify_capacity: bool,
    oauth_connections: Option<&Path>,
    reserve_mib: u64,
    home: &Path,
) -> Result<(CloudRoot, cloud::CloudPlanReport), String> {
    if !verify_capacity {
        let mut report = cloud::plan_cloud_archive_from_snapshot(source, root);
        attach_local_copy_prerequisites(&mut report, home);
        return Ok((root.clone(), report));
    }
    let observed_at_ms = cloud::system_now_ms();
    let capacity_snapshot = match collect_root_capacity(root, oauth_connections, observed_at_ms) {
        Ok(snapshot) => snapshot,
        Err(error) => provider_capacity::unavailable_capacity_from_error(
            root.provider,
            observed_at_ms,
            &error,
        ),
    };
    let refined_root =
        provider_capacity::root_with_verified_capacity_scope(root, &capacity_snapshot)?;
    let mut report = cloud::plan_cloud_archive_from_snapshot(source, &refined_root);
    attach_capacity_snapshot(&mut report, capacity_snapshot, reserve_mib)?;
    attach_local_copy_prerequisites(&mut report, home);
    Ok((refined_root, report))
}

#[cfg(not(coverage))]
fn attach_local_copy_prerequisites(report: &mut cloud::CloudPlanReport, home: &Path) {
    let runtime = provider_client_runtime::collect_provider_client_runtime(
        report.cloud_root.provider,
        cloud::system_now_ms(),
    );
    provider_client_runtime::attach_runtime_notice(&mut report.notices, &runtime);
    if report.capacity.as_ref().is_some_and(|assessment| {
        provider_capacity::native_personal_client_copy_capacity_exception(
            report.cloud_root.provider,
            report.cloud_root.account_scope,
            runtime.copy_prerequisite_met,
            &assessment.snapshot,
        )
    }) {
        report
            .notices
            .push("native-client-copy-capacity-unverified".into());
    }
    if report.cloud_root.provider == CloudProvider::Icloud {
        let health =
            icloud_sync_health::inspect_new_copy_admission(home, cloud::system_now_ms()).ok();
        icloud_sync_health::attach_new_copy_admission_notice(&mut report.notices, health.as_ref());
    } else {
        let global_sync =
            provider_global_sync::inspect_new_copy_admission(report.cloud_root.provider).ok();
        provider_global_sync::attach_new_copy_admission_notice(
            &mut report.notices,
            global_sync.as_ref(),
        );
    }
}

#[cfg(not(coverage))]
fn collect_receipt_sync_evidence(
    receipt: &CloudCopyReceipt,
    provider_object_id: Option<&str>,
    oauth_connections: Option<&Path>,
    home: &Path,
    confirmed_at_ms: u64,
    force_provider_api: bool,
) -> Result<disksage_lib::cloud_transfer::ProviderSyncEvidence, String> {
    let provider_object_id = provider_object_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match receipt.provider {
        CloudProvider::Icloud => {
            if provider_object_id.is_some() {
                return Err("icloud-provider-api-fallback-not-supported".into());
            }
            provider_sync::collect_icloud_sync_evidence(receipt, confirmed_at_ms)
        }
        CloudProvider::Onedrive | CloudProvider::GoogleDrive => {
            let fallback_requested = oauth_connections.is_some();
            if !force_provider_api {
                match provider_sync::collect_file_provider_sync_evidence(receipt, confirmed_at_ms) {
                    Ok(evidence) if evidence.sync_complete || !fallback_requested => {
                        return Ok(evidence);
                    }
                    Err(error) if !fallback_requested => return Err(error),
                    Ok(_) | Err(_) => {}
                }
            }
            {
                let connection_path = oauth_connections
                    .ok_or_else(|| "oauth-connections-path-missing".to_string())?;
                let selected_root = receipt_cloud_root(receipt, home)?;
                let access_token =
                    provider_oauth::refreshed_access_token(connection_path, &selected_root)?;
                match receipt.provider {
                    CloudProvider::Onedrive => {
                        if provider_object_id.is_some() {
                            return Err("onedrive-provider-object-id-not-accepted".into());
                        }
                        let locator = provider_api_client::onedrive_path_locator(
                            Path::new(&selected_root.path),
                            Path::new(&receipt.destination),
                        )?;
                        provider_api_client::collect_authenticated_provider_api_evidence_from_source(
                            receipt,
                            &locator,
                            access_token.as_str(),
                            &FixedHostProviderMetadataClient::default(),
                            confirmed_at_ms,
                        )
                    }
                    CloudProvider::GoogleDrive => {
                        let locator = provider_api_client::google_drive_path_locator(
                            Path::new(&selected_root.path),
                            Path::new(&receipt.destination),
                            provider_object_id
                                .ok_or_else(|| "provider-object-id-missing".to_string())?,
                        )?;
                        provider_api_client::collect_authenticated_google_drive_path_evidence_from_source(
                                receipt,
                                &locator,
                                access_token.as_str(),
                                &FixedHostProviderMetadataClient::default(),
                                confirmed_at_ms,
                            )
                    }
                    CloudProvider::Icloud => unreachable!(),
                }
            }
        }
    }
}

#[cfg(not(coverage))]
fn attest_receipt(
    path: &Path,
    evidence_dir: &Path,
    provider_object_id: Option<&str>,
    oauth_connections: Option<&Path>,
    home: &Path,
) -> Result<AttestationOutput, String> {
    attest_receipt_with_mode(
        path,
        evidence_dir,
        provider_object_id,
        oauth_connections,
        home,
        false,
    )
}

#[cfg(not(coverage))]
fn attest_receipt_with_mode(
    path: &Path,
    evidence_dir: &Path,
    provider_object_id: Option<&str>,
    oauth_connections: Option<&Path>,
    home: &Path,
    force_provider_api: bool,
) -> Result<AttestationOutput, String> {
    let receipt = cloud_transfer::read_immutable_receipt(path)?;
    let confirmed_at_ms = cloud::system_now_ms();
    let evidence = collect_receipt_sync_evidence(
        &receipt,
        provider_object_id,
        oauth_connections,
        home,
        confirmed_at_ms,
        force_provider_api,
    )?;
    let assessment = provider_sync::assess_provider_sync_timeliness(&receipt, &evidence)?;
    let (evidence_record, evidence_path) =
        provider_evidence::write_immutable_sync_evidence(evidence_dir, &evidence)?;
    let source_blocker = cloud_transfer::source_eviction_blocker(Path::new(&receipt.source));
    let (mut permit, mut blockers) =
        match cloud_transfer::approve_local_eviction(&receipt, &evidence_record) {
            Ok(permit) => (Some(permit), Vec::new()),
            Err(blockers) => (None, blockers),
        };
    if let Some(blocker) = source_blocker {
        permit = None;
        if !blockers.iter().any(|existing| existing == blocker) {
            blockers.push(blocker.into());
        }
    }
    let goal_state =
        cloud_transfer::CloudOffloadGoalState::after_attestation(&evidence, permit.is_some());
    let (adr_dir, goal_dir) = cloud_projection_dirs(evidence_dir);
    let mut adr = cloud_adr::snapshot_from_evidence(&evidence_record, goal_state, confirmed_at_ms);
    let mut goal = cloud_adr::goal_snapshot_from_evidence(
        &receipt,
        &evidence_record,
        goal_state,
        confirmed_at_ms,
    );
    if let Some(blocker) = source_blocker {
        goal.status = "blocked".into();
        goal.completion_gates.insert("source-present".into(), false);
        adr.decision = format!("{}-source-state-unverified", adr.decision);
        adr.consequences
            .push(format!("source-state-blocked:{blocker}"));
    }
    let provider_blocker = blockers
        .iter()
        .find(|existing| Some(existing.as_str()) != source_blocker)
        .map(String::as_str);
    let projection = cloud_adr::write_projection_pair_with_state_blockers_outcome(
        &adr_dir,
        &adr,
        &goal_dir,
        &goal,
        source_blocker,
        provider_blocker,
    );
    Ok(AttestationOutput {
        action: if force_provider_api {
            "attest-provider-api"
        } else {
            "attest-provider-native"
        },
        goal_state,
        goal_status: cloud_adr::read_goal_status(&goal_dir, &receipt.receipt_id)
            .ok()
            .flatten(),
        receipt_id: receipt.receipt_id,
        evidence,
        assessment,
        evidence_record,
        evidence_path: evidence_path.to_string_lossy().into_owned(),
        adr_path: projection
            .adr_path
            .map(|path| path.to_string_lossy().into_owned()),
        goal_path: projection
            .goal_path
            .map(|path| path.to_string_lossy().into_owned()),
        projection_warnings: projection.warnings,
        permit,
        blockers,
    })
}

#[cfg(not(coverage))]
fn stable_reconciliation_error(error: &str) -> String {
    let token = error.split(',').next().unwrap_or_default();
    if !token.is_empty()
        && token.len() <= 128
        && token
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        token.to_string()
    } else {
        "provider-attestation-failed".into()
    }
}

#[cfg(not(coverage))]
fn copy_candidate_via_provider_api(
    candidate: &cloud::CloudCandidate,
    selected: &CloudRoot,
    report: &cloud::CloudPlanReport,
    receipt_dir: &Path,
    review_decision: Option<&CloudReviewDecision>,
    exact_confirmation_phrase: &str,
    approved_by: &str,
    rationale: &str,
    oauth_connections: &Path,
    capacity_reserve_mib: u64,
    home: &Path,
) -> Result<ProviderApiCopyOutput, String> {
    if selected.provider == CloudProvider::Icloud {
        return Err("provider-api-icloud-unsupported".into());
    }
    let connection = provider_oauth::connection_for_root(
        &provider_oauth::load_connections(oauth_connections)?,
        selected,
    )?;
    if !provider_oauth::scope_allows_write(&connection) {
        return Err("provider-oauth-write-scope-required".into());
    }
    let capacity_snapshot = report
        .capacity
        .as_ref()
        .map(|assessment| assessment.snapshot.clone())
        .ok_or_else(|| "cloud-capacity-verification-required".to_string())?;
    let capacity = provider_capacity::assess_capacity(
        capacity_snapshot,
        candidate.bytes,
        candidate.bytes,
        capacity_reserve_mib.saturating_mul(1024 * 1024),
    );
    if capacity.can_fit != Some(true) {
        return Err(if capacity.blockers.is_empty() {
            "cloud-capacity-verification-required".into()
        } else {
            capacity.blockers.join(",")
        });
    }

    let copy_approval = cloud_transfer::create_cloud_copy_approval(
        candidate,
        selected,
        cloud_transfer::CloudCopyApprovalAction::CopyOnly,
        cloud::system_now_ms(),
        approved_by,
        rationale,
        exact_confirmation_phrase,
    )?;
    let copied_at_ms = cloud::system_now_ms();
    let (receipt, source_hashes) = cloud_transfer::prepare_provider_api_source_receipt(
        candidate,
        selected,
        review_decision,
        &copy_approval,
        copied_at_ms,
    )?;
    let access_token = provider_oauth::refreshed_access_token(oauth_connections, selected)?;
    let cancel = AtomicBool::new(false);
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(PROVIDER_API_COPY_DEADLINE_SECS))
        .ok_or_else(|| "cloud-copy-deadline-unavailable".to_string())?;
    let control = provider_api_write::ProviderUploadControl::new(&cancel, deadline);
    let failure_dir = receipt_dir
        .parent()
        .unwrap_or(receipt_dir)
        .join("cloud-copy-failures");
    let upload = provider_api_write::upload_file(
        selected.provider,
        Path::new(&selected.path),
        Path::new(&candidate.dst),
        Path::new(&candidate.src),
        candidate.bytes,
        access_token.as_str(),
        &control,
    );
    let upload = match upload {
        Ok(upload) => upload,
        Err(error) => return Err(cloud_transfer::journal_provider_api_copy_failure(
            candidate, &error, &failure_dir,
        )),
    };
    if let Err(error) =
        cloud_transfer::verify_provider_api_source_unchanged(candidate, &source_hashes)
    {
        let cleanup = provider_api_write::delete_uploaded_object(
            selected.provider,
            &upload.object_id,
            access_token.as_str(),
        );
        let error = match cleanup {
            Ok(()) => error,
            Err(cleanup_error) => {
                format!("{error},provider-api-upload-cleanup-failed:{cleanup_error}")
            }
        };
        return Err(cloud_transfer::journal_provider_api_copy_failure(
            candidate, &error, &failure_dir,
        ));
    }

    let receipt_path = match cloud_transfer::write_provider_api_receipt(&receipt, receipt_dir) {
        Ok(path) => path,
        Err(error) => {
            let cleanup = provider_api_write::delete_uploaded_object(
                selected.provider,
                &upload.object_id,
                access_token.as_str(),
            );
            let error = match cleanup {
                Ok(()) => error,
                Err(cleanup_error) => {
                    format!("{error},provider-api-upload-cleanup-failed:{cleanup_error}")
                }
            };
            return Err(cloud_transfer::journal_provider_api_copy_failure(
                candidate, &error, &failure_dir,
            ));
        }
    };

    let evidence_dir = receipt_dir
        .parent()
        .unwrap_or(receipt_dir)
        .join("cloud-provider-evidence");
    let (adr_dir, goal_dir) = cloud_projection_dirs(receipt_dir);
    let updated_at_ms = cloud::system_now_ms();
    let adr = cloud_adr::initial_adr_snapshot(&receipt, updated_at_ms);
    let goal = cloud_adr::initial_goal_snapshot(&receipt, updated_at_ms);
    let (initial_adr_path, initial_goal_path, mut projection_warnings) =
        cloud_adr::write_projection_pair(&adr_dir, &adr, &goal_dir, &goal);
    let mut adr_path = initial_adr_path.map(|path| path.to_string_lossy().into_owned());
    let mut goal_path = initial_goal_path.map(|path| path.to_string_lossy().into_owned());
    let receipt_id = receipt.receipt_id.clone();
    let mut goal_state = cloud_transfer::CloudOffloadGoalState::CopyVerified;
    let mut evidence_path = None;
    let mut permit = None;
    let mut blockers = Vec::new();
    let provider_object_id = upload.object_id;
    let attest_object_id =
        (selected.provider == CloudProvider::GoogleDrive).then(|| provider_object_id.clone());
    match attest_receipt_with_mode(
        &receipt_path,
        &evidence_dir,
        attest_object_id.as_deref(),
        Some(oauth_connections),
        home,
        true,
    ) {
        Ok(attestation) => {
            goal_state = attestation.goal_state;
            evidence_path = Some(attestation.evidence_path);
            permit = attestation.permit;
            blockers = attestation.blockers;
            adr_path = attestation.adr_path;
            goal_path = attestation.goal_path;
            projection_warnings.extend(attestation.projection_warnings);
        }
        Err(error) => {
            let provider_blocker = stable_reconciliation_error(&error);
            let projection_outcome =
                cloud_adr::ensure_initial_projection_pair_with_provider_state_outcome(
                    &receipt,
                    &adr_dir,
                    &goal_dir,
                    cloud::system_now_ms(),
                    &provider_blocker,
                );
            if let Some(path) = projection_outcome.adr_path {
                adr_path = Some(path.to_string_lossy().into_owned());
            }
            if let Some(path) = projection_outcome.goal_path {
                goal_path = Some(path.to_string_lossy().into_owned());
            }
            projection_warnings.extend(projection_outcome.warnings);
            projection_warnings.push(format!(
                "provider-attestation-incomplete:{provider_blocker}"
            ));
        }
    }

    Ok(ProviderApiCopyOutput {
        action: "copy-via-provider-api",
        goal_state,
        goal_status: cloud_adr::read_goal_status(&goal_dir, &receipt_id)
            .ok()
            .flatten(),
        receipt,
        receipt_path: receipt_path.to_string_lossy().into_owned(),
        provider_object_id,
        evidence_path,
        adr_path: adr_path.or_else(|| {
            Some(
                adr_dir
                    .join(format!("{}-latest.json", receipt_id))
                    .to_string_lossy()
                    .into_owned(),
            )
        }),
        goal_path: goal_path.or_else(|| {
            Some(
                goal_dir
                    .join(format!("{}-latest.json", receipt_id))
                    .to_string_lossy()
                    .into_owned(),
            )
        }),
        projection_warnings,
        permit,
        blockers,
    })
}

/// Re-attest every persisted receipt and refresh only local provider evidence and ADR/Goal
/// projections. This is the headless equivalent of the GUI reconciliation loop; it never writes
/// to a cloud provider and never evicts a source file.
#[cfg(not(coverage))]
fn reconcile_receipts(
    receipt_dir: &Path,
    evidence_dir: &Path,
    provider_object_id: Option<&str>,
    oauth_connections: Option<&Path>,
    home: &Path,
    generated_at_ms: u64,
) -> Result<ReceiptReconciliationReport, String> {
    let reconciliation_started = Instant::now();
    let mut report = audit_receipts(receipt_dir, Some(evidence_dir), generated_at_ms)?;
    report.notices = vec![
        "provider-attestation-attempted",
        "local-provider-evidence-write",
        "dynamic-adr-goal-projection-write",
        "immutable-receipts-remain-authority",
        "no-cloud-write",
        "no-local-eviction",
    ];
    let (adr_dir, goal_dir) = cloud_projection_dirs(evidence_dir);
    let mut paths = std::fs::read_dir(receipt_dir)
        .map_err(|_| "receipt-directory-read-failed".to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    if paths.len() > MAX_RECONCILIATION_RECEIPTS {
        return Err("receipt-directory-entry-limit-exceeded".into());
    }
    let receipt_paths = paths
        .into_iter()
        .filter(|path| {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            regular_file_state(path) == "present" && file_name.ends_with(".json")
        })
        .collect::<Vec<_>>();
    for (index, path) in receipt_paths.iter().enumerate() {
        if index >= MAX_RECONCILIATION_ATTESTATIONS
            || reconciliation_started.elapsed() >= RECONCILIATION_MAX_DURATION
        {
            report.unprocessed_count = receipt_paths.len().saturating_sub(index) as u64;
            report.incomplete_reconciliation = report.unprocessed_count > 0;
            if report.incomplete_reconciliation {
                let notice = if index >= MAX_RECONCILIATION_ATTESTATIONS {
                    "reconciliation-entry-limit"
                } else {
                    "reconciliation-time-limit"
                };
                report.notices.push(notice);
            }
            break;
        }
        let Ok(receipt) = cloud_transfer::read_immutable_receipt(&path) else {
            continue;
        };
        let Some(entry_index) = report
            .entries
            .iter()
            .position(|entry| entry.receipt_id.as_deref() == Some(receipt.receipt_id.as_str()))
        else {
            continue;
        };
        report.attestation_attempted_count = report.attestation_attempted_count.saturating_add(1);
        match attest_receipt(
            &path,
            evidence_dir,
            provider_object_id,
            oauth_connections,
            home,
        ) {
            Ok(attestation) => {
                report.provider_evidence_written_count =
                    report.provider_evidence_written_count.saturating_add(1);
                report.mutation_performed = true;
                if attestation.goal_state
                    == cloud_transfer::CloudOffloadGoalState::PendingProviderSync
                {
                    report.pending_provider_sync_count =
                        report.pending_provider_sync_count.saturating_add(1);
                }
                if attestation.permit.is_some() {
                    report.eviction_ready_count = report.eviction_ready_count.saturating_add(1);
                }
                let entry = &mut report.entries[entry_index];
                entry.goal_state = Some(attestation.goal_state);
                entry.provider_sync_state = Some(attestation.evidence.sync_state);
                entry.eviction_permit = attestation.permit.is_some();
                entry.attestation_error = None;
                entry.issues.extend(attestation.blockers);
                entry.issues.extend(
                    attestation
                        .projection_warnings
                        .into_iter()
                        .map(|warning| format!("projection-{warning}")),
                );
                entry.adr_projection_state = Some(
                    projection_state(
                        &adr_dir.join(format!("{}-latest.json", receipt.receipt_id)),
                        "adr",
                        &receipt.receipt_id,
                    )
                    .into(),
                );
                entry.goal_projection_state = Some(
                    projection_state(
                        &goal_dir.join(format!("{}-latest.json", receipt.receipt_id)),
                        "goal",
                        &receipt.receipt_id,
                    )
                    .into(),
                );
                entry.goal_status = cloud_adr::read_goal_status(&goal_dir, &receipt.receipt_id)
                    .ok()
                    .flatten();
                entry.evidence_record_count =
                    evidence_record_count(&[evidence_dir.to_path_buf()], &receipt.receipt_id);
            }
            Err(error) => {
                let attestation_error = stable_reconciliation_error(&error);
                let projection_outcome =
                    cloud_adr::ensure_initial_projection_pair_with_provider_state_outcome(
                        &receipt,
                        &adr_dir,
                        &goal_dir,
                        generated_at_ms,
                        &attestation_error,
                    );
                let projection_warnings = projection_outcome.warnings;
                report.mutation_performed |= projection_outcome.wrote;
                let projection =
                    cloud_adr::read_projection_state(&receipt.receipt_id, &adr_dir, &goal_dir);
                let entry = &mut report.entries[entry_index];
                entry.attestation_error = Some(attestation_error);
                entry.issues.push("provider-attestation-incomplete".into());
                if let Some(blocker) =
                    cloud_transfer::source_eviction_blocker(Path::new(&receipt.source))
                {
                    entry.issues.push(blocker.into());
                }
                if !projection_warnings.is_empty() {
                    entry
                        .issues
                        .push("dynamic-projection-update-incomplete".into());
                }
                entry.issues.extend(
                    projection_warnings
                        .into_iter()
                        .map(|warning| format!("projection-{warning}")),
                );
                match projection {
                    Ok(Some(state)) => {
                        entry.goal_state = Some(state.goal_state);
                        entry.provider_sync_state = Some(state.provider_sync_state);
                        entry.eviction_permit = false;
                        entry.issues.push("projection-state-not-revalidated".into());
                        if state.goal_state
                            == cloud_transfer::CloudOffloadGoalState::PendingProviderSync
                        {
                            report.pending_provider_sync_count =
                                report.pending_provider_sync_count.saturating_add(1);
                        }
                    }
                    Ok(None) => entry
                        .issues
                        .push("dynamic-projection-state-unavailable".into()),
                    Err(_) => entry
                        .issues
                        .push("dynamic-projection-state-unavailable".into()),
                }
                entry.adr_projection_state = Some(
                    projection_state(
                        &adr_dir.join(format!("{}-latest.json", receipt.receipt_id)),
                        "adr",
                        &receipt.receipt_id,
                    )
                    .into(),
                );
                entry.goal_projection_state = Some(
                    projection_state(
                        &goal_dir.join(format!("{}-latest.json", receipt.receipt_id)),
                        "goal",
                        &receipt.receipt_id,
                    )
                    .into(),
                );
                entry.goal_status = cloud_adr::read_goal_status(&goal_dir, &receipt.receipt_id)
                    .ok()
                    .flatten();
                entry.evidence_record_count =
                    evidence_record_count(&[evidence_dir.to_path_buf()], &receipt.receipt_id);
            }
        }
        refresh_projection_issues(&mut report.entries[entry_index]);
    }
    report.incomplete_projection_count = report
        .entries
        .iter()
        .filter(|entry| {
            entry.adr_projection_state.as_deref() != Some("valid")
                || entry.goal_projection_state.as_deref() != Some("valid")
        })
        .count() as u64;
    Ok(report)
}

#[cfg(not(coverage))]
fn evict_native_receipt(
    path: &Path,
    confirmation_receipt_id: &str,
    eviction_dir: &Path,
    approval_dir: &Path,
    journal_path: &Path,
    evidence_dir: &Path,
    approved_by: &str,
    rationale: &str,
    provider_object_id: Option<&str>,
    oauth_connections: Option<&Path>,
    home: &Path,
) -> Result<EvictionOutput, String> {
    let receipt = cloud_transfer::read_immutable_receipt(path)?;
    if confirmation_receipt_id != receipt.receipt_id {
        return Err("eviction-confirmation-receipt-id-mismatch".into());
    }
    let confirmed_at_ms = cloud::system_now_ms();
    let evidence = collect_receipt_sync_evidence(
        &receipt,
        provider_object_id,
        oauth_connections,
        home,
        confirmed_at_ms,
        false,
    )?;
    let (evidence_record, evidence_path) =
        provider_evidence::write_immutable_sync_evidence(evidence_dir, &evidence)?;
    if let Some(blocker) = cloud_transfer::source_eviction_blocker(Path::new(&receipt.source)) {
        return Err(blocker.into());
    }
    let permit = cloud_transfer::approve_local_eviction(&receipt, &evidence_record)
        .map_err(|blockers| blockers.join(","))?;
    let active_use_observed_at_ms = cloud::system_now_ms();
    let active_use = cloud_local_eviction::observe_path_active_use(Path::new(&receipt.source));
    let approved_at_ms = cloud::system_now_ms();
    let approval = cloud_eviction::create_source_eviction_approval(
        &receipt,
        &permit,
        confirmation_receipt_id,
        approved_at_ms,
        approved_by,
        rationale,
        active_use_observed_at_ms,
        active_use,
    )?;
    let approval_path =
        cloud_eviction::write_immutable_source_eviction_approval(approval_dir, &approval)?;
    let eviction = cloud_eviction::evict_source_with_human_approval(
        &receipt,
        &permit,
        &approval,
        confirmation_receipt_id,
        eviction_dir,
        journal_path,
        cloud::system_now_ms(),
    )?;
    let goal_state = cloud_transfer::CloudOffloadGoalState::SourceEvicted;
    let updated_at_ms = cloud::system_now_ms();
    let (adr_dir, goal_dir) = cloud_projection_dirs(evidence_dir);
    let adr = cloud_adr::snapshot_from_evidence(&evidence_record, goal_state, updated_at_ms);
    let goal = cloud_adr::goal_snapshot_from_evidence(
        &receipt,
        &evidence_record,
        goal_state,
        updated_at_ms,
    );
    let (adr_path, goal_path, projection_warnings) =
        cloud_adr::write_projection_pair(&adr_dir, &adr, &goal_dir, &goal);
    Ok(EvictionOutput {
        action: "attest-and-trash-verified-cloud-source",
        goal_state,
        receipt_id: receipt.receipt_id,
        evidence,
        evidence_record,
        evidence_path: evidence_path.to_string_lossy().into_owned(),
        permit,
        approval,
        approval_path: approval_path.to_string_lossy().into_owned(),
        eviction,
        adr_path: adr_path.map(|path| path.to_string_lossy().into_owned()),
        goal_path: goal_path.map(|path| path.to_string_lossy().into_owned()),
        projection_warnings,
    })
}

#[cfg(not(coverage))]
fn home_dir() -> Result<PathBuf, String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .map_err(|_| "HOME/USERPROFILE을 찾을 수 없음".into())
}

#[cfg(not(coverage))]
fn select_root(roots: &[CloudRoot], args: &Args) -> Result<CloudRoot, String> {
    let matches: Vec<&CloudRoot> = roots
        .iter()
        .filter(|root| {
            args.cloud_root
                .as_ref()
                .map(|path| cloud::cloud_root_path_matches(Path::new(&root.path), path))
                .unwrap_or(true)
                && args.provider.map(|p| p == root.provider).unwrap_or(true)
        })
        .collect();
    match matches.as_slice() {
        [only] => Ok((*only).clone()),
        [] => Err("조건과 일치하는 탐지된 클라우드 루트가 없음 (--list-roots로 확인)".into()),
        _ => Err("클라우드 루트가 여러 개임; --cloud-root로 하나를 선택해야 함".into()),
    }
}

#[cfg(not(coverage))]
fn run() -> Result<(), String> {
    let home = home_dir()?;
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw, &home)?;
    validate_action_args(&args)?;
    if args.reconcile_receipts {
        let report = reconcile_receipts(
            args.receipt_dir
                .as_deref()
                .ok_or_else(|| "--reconcile-receipts에는 --receipt-dir이 필요함".to_string())?,
            args.evidence_dir
                .as_deref()
                .ok_or_else(|| "--reconcile-receipts에는 --evidence-dir이 필요함".to_string())?,
            args.provider_object_id.as_deref(),
            args.oauth_connections.as_deref(),
            &home,
            cloud::system_now_ms(),
        )?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if args.audit_receipts {
        let report = audit_receipts(
            args.receipt_dir
                .as_deref()
                .ok_or_else(|| "--audit-receipts에는 --receipt-dir이 필요함".to_string())?,
            args.evidence_dir.as_deref(),
            cloud::system_now_ms(),
        )?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if let Some(receipt_path) = &args.export_naruon_lineage {
        let receipt = cloud_transfer::read_immutable_receipt(receipt_path)?;
        let evidence = args
            .naruon_sync_evidence
            .as_deref()
            .map(provider_evidence::read_immutable_sync_evidence)
            .transpose()?;
        let envelope = naruon_lineage::export_naruon_file_lineage(&receipt, evidence.as_ref())?;
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if let Some(receipt_path) = &args.evict_receipt {
        let output = evict_native_receipt(
            receipt_path,
            args.confirm_receipt_id
                .as_deref()
                .ok_or_else(|| "--confirm-receipt-id가 필요함".to_string())?,
            args.eviction_dir
                .as_deref()
                .ok_or_else(|| "--eviction-dir이 필요함".to_string())?,
            args.eviction_approval_dir
                .as_deref()
                .ok_or_else(|| "--eviction-approval-dir이 필요함".to_string())?,
            args.journal_path
                .as_deref()
                .ok_or_else(|| "--journal-path가 필요함".to_string())?,
            args.evidence_dir
                .as_deref()
                .ok_or_else(|| "--evidence-dir이 필요함".to_string())?,
            args.reviewed_by
                .as_deref()
                .ok_or_else(|| "--reviewed-by가 필요함".to_string())?,
            args.review_rationale
                .as_deref()
                .ok_or_else(|| "--review-rationale가 필요함".to_string())?,
            args.provider_object_id.as_deref(),
            args.oauth_connections.as_deref(),
            &home,
        )?;
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if let Some(receipt_path) = &args.attest_receipt {
        println!(
            "{}",
            serde_json::to_string_pretty(&attest_receipt(
                receipt_path,
                args.evidence_dir
                    .as_deref()
                    .ok_or_else(|| "--evidence-dir이 필요함".to_string())?,
                args.provider_object_id.as_deref(),
                args.oauth_connections.as_deref(),
                &home,
            )?)
            .map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    let discovery = cloud::discover_cloud_roots_report(&home);
    if args.inspect_roots {
        println!(
            "{}",
            serde_json::to_string_pretty(&discovery).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let roots = discovery.roots;
    if args.list_roots {
        println!(
            "{}",
            serde_json::to_string_pretty(&roots).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    cloud::validate_source_root_readable(&args.root)?;
    let selected_roots = if args.all_readable_roots {
        let selected = roots
            .iter()
            .filter(|root| root.readable)
            .cloned()
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return Err("재검증할 수 있는 읽기 가능 클라우드 루트가 없음".into());
        }
        selected
    } else {
        vec![select_root(&roots, &args)?]
    };
    for selected in &selected_roots {
        cloud::validate_cloud_root_readable(selected)?;
    }
    let excluded: Vec<PathBuf> = roots.iter().map(|r| PathBuf::from(&r.path)).collect();
    if excluded
        .iter()
        .any(|cloud_root| args.root.starts_with(cloud_root))
    {
        return Err("이미 클라우드 안에 있는 경로는 오프로드 원본으로 사용할 수 없음".into());
    }
    let collection = cloud::collect_archive_files_bounded(
        &args.root,
        &excluded,
        cloud::ARCHIVE_SCAN_MAX_ENTRIES,
        cloud::ARCHIVE_SCAN_MAX_DURATION,
    );
    let snapshot = cloud::prepare_cloud_archive_source_from_collection(
        &collection,
        &args.root,
        cloud::system_now_ms(),
        CloudPlanOptions {
            min_size_bytes: args.min_size_mib.saturating_mul(1024 * 1024),
            min_age_days: args.min_age_days,
            limit: args.limit.clamp(1, 1_000),
        },
    );
    if args.all_readable_roots {
        let mut summaries = Vec::with_capacity(selected_roots.len());
        for selected in &selected_roots {
            let (_, report) = plan_with_optional_capacity(
                &snapshot,
                selected,
                args.verify_capacity,
                args.oauth_connections.as_deref(),
                args.capacity_reserve_mib,
                &home,
            )?;
            summaries.push(match args.review_reason_set.as_deref() {
                Some(reasons) => review_batch_summary(&report, reasons)?,
                None => compact_decision_summary(&report),
            });
        }
        let capacity_notice = if args.verify_capacity {
            "cloud-capacity-assessed-per-destination"
        } else {
            "cloud-capacity-unverified"
        };
        let output = serde_json::json!({
            "schema_version": 3,
            "output_mode": "multicloud-decision-summary",
            "source_snapshot": {
                "candidate_count": snapshot.candidate_count(),
                "candidate_bytes": snapshot.candidate_bytes(),
                "reused_for_destination_count": selected_roots.len(),
                "content_metadata_probed_once": true,
                "duplicate_content_hashed_once": true,
            },
            "destinations": summaries,
            "notices": [
                "dry-run-only",
                "destination-state-revalidated-per-plan",
                "source-stat-revalidated-per-plan",
                capacity_notice,
                "cloud-sync-unverified",
            ],
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    let selected = selected_roots
        .into_iter()
        .next()
        .ok_or_else(|| "선택된 클라우드 루트가 없음".to_string())?;
    let capacity_required_for_plan = args.verify_capacity
        || args.copy_fingerprint.is_some()
        || args.provider_api_copy_fingerprint.is_some();
    let (selected, mut report) = plan_with_optional_capacity(
        &snapshot,
        &selected,
        capacity_required_for_plan,
        args.oauth_connections.as_deref(),
        args.capacity_reserve_mib,
        &home,
    )?;
    if args.export_naruon_capacity {
        let envelope = naruon_capacity::export_naruon_cloud_capacity_assessment(&report)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if args.export_naruon_copy_readiness {
        let observed_at_ms = cloud::system_now_ms();
        let runtime = provider_client_runtime::collect_provider_client_runtime(
            selected.provider,
            observed_at_ms,
        );
        let icloud_health = if selected.provider == CloudProvider::Icloud {
            icloud_sync_health::inspect_new_copy_admission(&home, observed_at_ms).ok()
        } else {
            None
        };
        let provider_global_sync = if selected.provider == CloudProvider::Icloud {
            None
        } else {
            provider_global_sync::inspect_new_copy_admission(selected.provider).ok()
        };
        if selected.provider == CloudProvider::Icloud {
            attach_pre_copy_evidence_cohort(&mut report, &runtime, icloud_health.as_ref());
        }
        let envelope =
            naruon_cloud_copy_readiness::export_naruon_cloud_copy_readiness_with_global_sync(
                &report,
                &runtime,
                icloud_health.as_ref(),
                provider_global_sync.as_ref(),
            )?;
        if let Some(output_path) = &args.naruon_copy_readiness_output {
            let value = serde_json::to_value(&envelope)
                .map_err(|_| "naruon-copy-readiness-output-json-invalid".to_string())?;
            write_private_review_dossier(output_path, &value)?;
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if args.export_semantic_catalog {
        let batch = semantic_catalog::export_semantic_catalog_candidate_batch(&report)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&batch).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if let (Some(redundant_prefix), Some(kind)) = (
        args.exact_duplicate_review_prefix.as_deref(),
        args.exact_duplicate_kind,
    ) {
        let output = exact_duplicate_review_batch(&report, redundant_prefix, kind)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if let Some(candidate_fingerprint) = &args.review_candidate_fingerprint {
        let review_fingerprint = args
            .review_fingerprint
            .as_deref()
            .ok_or_else(|| "--review-fingerprint가 필요함".to_string())?;
        let matches: Vec<_> = report
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.metadata_fingerprint == *candidate_fingerprint
                    && candidate.review_fingerprint == review_fingerprint
            })
            .collect();
        let candidate = match matches.as_slice() {
            [only] => *only,
            [] => return Err("현재 fresh plan에 review fingerprint가 일치하는 후보가 없음".into()),
            _ => return Err("현재 fresh plan에서 review fingerprint가 중복됨".into()),
        };
        let disposition = args
            .review_disposition
            .ok_or_else(|| "--review-disposition이 필요함".to_string())?;
        let decision = cloud_review::create_attributed_decision(
            candidate,
            disposition,
            cloud::system_now_ms(),
            args.reviewed_by
                .as_deref()
                .ok_or_else(|| "--reviewed-by가 필요함".to_string())?,
            args.review_rationale
                .as_deref()
                .ok_or_else(|| "--review-rationale가 필요함".to_string())?,
        )?;
        let decision_path = cloud_review::write_immutable_decision(
            args.review_dir
                .as_deref()
                .ok_or_else(|| "--review-dir이 필요함".to_string())?,
            &decision,
        )?;
        println!(
            "{}",
            serde_json::to_string_pretty(&ReviewOutput {
                action: "review",
                decision,
                decision_path: decision_path.to_string_lossy().into_owned(),
            })
            .map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if let Some(candidate_fingerprint) = &args.provider_api_copy_fingerprint {
        let matches: Vec<_> = report
            .candidates
            .iter()
            .filter(|candidate| candidate.metadata_fingerprint == *candidate_fingerprint)
            .collect();
        let candidate = match matches.as_slice() {
            [only] => *only,
            [] => return Err("현재 fresh plan에 fingerprint가 일치하는 후보가 없음".into()),
            _ => return Err("현재 fresh plan에서 fingerprint가 중복됨".into()),
        };
        let receipt_dir = args
            .receipt_dir
            .as_deref()
            .ok_or_else(|| "--receipt-dir이 필요함".to_string())?;
        let review_decision = if candidate.requires_review {
            args.review_dir
                .as_deref()
                .map(cloud_review::load_latest_decisions)
                .transpose()?
                .unwrap_or_default()
                .into_iter()
                .find(|decision| decision.candidate_fingerprint == candidate.metadata_fingerprint)
        } else {
            None
        };
        let output = copy_candidate_via_provider_api(
            candidate,
            &selected,
            &report,
            receipt_dir,
            review_decision.as_ref(),
            args.confirm_copy_phrase
                .as_deref()
                .ok_or_else(|| "--confirm-copy-phrase가 필요함".to_string())?,
            args.reviewed_by
                .as_deref()
                .ok_or_else(|| "--reviewed-by가 필요함".to_string())?,
            args.review_rationale
                .as_deref()
                .ok_or_else(|| "--review-rationale가 필요함".to_string())?,
            args.oauth_connections
                .as_deref()
                .ok_or_else(|| "--oauth-connections가 필요함".to_string())?,
            args.capacity_reserve_mib,
            &home,
        )?;
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    let receipt_action = args
        .copy_fingerprint
        .as_ref()
        .map(|fingerprint| (fingerprint, false))
        .or_else(|| {
            args.adopt_existing_fingerprint
                .as_ref()
                .map(|fingerprint| (fingerprint, true))
        });
    if let Some((fingerprint, adopt_existing)) = receipt_action {
        let matches: Vec<_> = report
            .candidates
            .iter()
            .filter(|candidate| candidate.metadata_fingerprint == *fingerprint)
            .collect();
        let candidate = match matches.as_slice() {
            [only] => *only,
            [] => return Err("현재 fresh plan에 fingerprint가 일치하는 후보가 없음".into()),
            _ => return Err("현재 fresh plan에서 fingerprint가 중복됨".into()),
        };
        let receipt_dir = args
            .receipt_dir
            .as_deref()
            .ok_or_else(|| "--receipt-dir이 필요함".to_string())?;
        let review_decision = if candidate.requires_review {
            args.review_dir
                .as_deref()
                .map(cloud_review::load_latest_decisions)
                .transpose()?
                .unwrap_or_default()
                .into_iter()
                .find(|decision| decision.candidate_fingerprint == candidate.metadata_fingerprint)
        } else {
            None
        };
        let action = if adopt_existing {
            cloud_transfer::CloudCopyApprovalAction::AdoptExistingCopy
        } else {
            cloud_transfer::CloudCopyApprovalAction::CopyOnly
        };
        let action_at_ms = cloud::system_now_ms();
        let copy_approval = cloud_transfer::create_cloud_copy_approval(
            candidate,
            &selected,
            action,
            action_at_ms,
            args.reviewed_by
                .as_deref()
                .ok_or_else(|| "--reviewed-by가 필요함".to_string())?,
            args.review_rationale
                .as_deref()
                .ok_or_else(|| "--review-rationale가 필요함".to_string())?,
            args.confirm_copy_phrase
                .as_deref()
                .ok_or_else(|| "--confirm-copy-phrase가 필요함".to_string())?,
        )?;
        if !adopt_existing {
            let runtime = provider_client_runtime::require_provider_client_runtime(
                selected.provider,
                cloud::system_now_ms(),
            )?;
            if selected.provider == CloudProvider::Icloud {
                let health =
                    icloud_sync_health::inspect_new_copy_admission(&home, cloud::system_now_ms())
                        .map_err(|_| "icloud-new-copy-admission-evidence-unavailable".to_string())?;
                icloud_sync_health::require_new_copy_admission(&health)?;
            } else {
                let global_sync =
                    provider_global_sync::inspect_new_copy_admission(selected.provider)
                        .map_err(|_| "provider-global-sync-evidence-unavailable".to_string())?;
                provider_global_sync::require_new_copy_admission(&global_sync)?;
            }
            let capacity_snapshot = report
                .capacity
                .as_ref()
                .map(|assessment| assessment.snapshot.clone())
                .ok_or_else(|| "cloud-capacity-verification-required".to_string())?;
            let assessment = provider_capacity::assess_capacity(
                capacity_snapshot,
                candidate.bytes,
                candidate.bytes,
                args.capacity_reserve_mib.saturating_mul(1024 * 1024),
            );
            let native_client_mode =
                provider_capacity::native_personal_client_copy_capacity_exception(
                    selected.provider,
                    selected.account_scope,
                    runtime.copy_prerequisite_met,
                    &assessment.snapshot,
                );
            if assessment.can_fit != Some(true) && !native_client_mode {
                return Err(if assessment.blockers.is_empty() {
                    "cloud-capacity-verification-required".into()
                } else {
                    assessment.blockers.join(",")
                });
            }
        }
        let (receipt, receipt_path) = if adopt_existing {
            cloud_transfer::adopt_existing_cloud_copy_with_approval(
                candidate,
                &selected,
                receipt_dir,
                review_decision.as_ref(),
                &copy_approval,
            )?
        } else {
            cloud_transfer::prepare_cloud_copy_with_approval(
                candidate,
                &selected,
                receipt_dir,
                review_decision.as_ref(),
                &copy_approval,
            )?
        };
        let (adr_dir, goal_dir) = cloud_projection_dirs(receipt_dir);
        let projection_updated_at_ms = cloud::system_now_ms();
        let adr = cloud_adr::initial_adr_snapshot(&receipt, projection_updated_at_ms);
        let goal = cloud_adr::initial_goal_snapshot(&receipt, projection_updated_at_ms);
        let (adr_path, goal_path, projection_warnings) =
            cloud_adr::write_projection_pair(&adr_dir, &adr, &goal_dir, &goal);
        println!(
            "{}",
            serde_json::to_string_pretty(&CopyOutput {
                action: if adopt_existing {
                    "adopt-existing-copy"
                } else {
                    "copy-only"
                },
                goal_state: cloud_transfer::CloudOffloadGoalState::CopyVerified,
                goal_status: cloud_adr::read_goal_status(&goal_dir, &receipt.receipt_id)
                    .ok()
                    .flatten(),
                receipt,
                receipt_path: receipt_path.to_string_lossy().into_owned(),
                adr_path: adr_path.map(|path| path.to_string_lossy().into_owned()),
                goal_path: goal_path.map(|path| path.to_string_lossy().into_owned()),
                projection_warnings,
            })
            .map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if args.decision_summary {
        let mut summary = match args.review_reason_set.as_deref() {
            Some(reasons) => review_batch_summary(&report, reasons)?,
            None => decision_summary(&report),
        };
        if let Some(output_path) = &args.private_candidate_inspection_output {
            let dossier = private_candidate_inspection_dossier(&report);
            let (sha256, bytes) = write_private_review_dossier(output_path, &dossier)?;
            summary
                .as_object_mut()
                .ok_or_else(|| "decision summary JSON object가 아님".to_string())?
                .insert(
                    "private_candidate_inspection_dossier".into(),
                    serde_json::json!({
                        "written": true,
                        "bytes": bytes,
                        "sha256": sha256,
                        "unix_mode": "0600",
                        "create_new": true,
                        "contains_sensitive_local_metadata": true,
                        "includes_blocked_candidates": true,
                        "is_approval": false,
                        "cloud_write_executed": false,
                        "source_eviction_authorized": false,
                    }),
                );
        }
        if let Some(output_path) = &args.private_review_output {
            let reasons = args
                .review_reason_set
                .as_deref()
                .ok_or_else(|| "private review reason set이 없음".to_string())?;
            let dossier = private_review_dossier(&report, reasons)?;
            let (sha256, bytes) = write_private_review_dossier(output_path, &dossier)?;
            summary
                .as_object_mut()
                .ok_or_else(|| "review summary JSON object가 아님".to_string())?
                .insert(
                    "private_review_dossier".into(),
                    serde_json::json!({
                        "written": true,
                        "bytes": bytes,
                        "sha256": sha256,
                        "unix_mode": "0600",
                        "create_new": true,
                        "contains_sensitive_local_metadata": true,
                        "is_approval": false,
                    }),
                );
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    }
    Ok(())
}

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage cloud planner: {error}");
        std::process::exit(2);
    }
}

#[cfg(coverage)]
fn main() {}

#[cfg(all(test, coverage))]
mod coverage_tests {
    #[test]
    fn noop_main_runs() {
        super::main();
    }
}

#[cfg(all(test, not(coverage)))]
mod tests {
    use super::*;

    #[test]
    fn archive_kind_label_includes_sensitive_config_wire_name() {
        assert_eq!(
            archive_kind_label(ArchiveKind::SensitiveConfig),
            "sensitive-config"
        );
    }

    #[test]
    fn parses_defaults_and_explicit_values() {
        let defaults = parse_args(&[], Path::new("/home/test")).unwrap();
        assert_eq!(defaults.root, PathBuf::from("/home/test"));
        assert_eq!(defaults.min_size_mib, 256);
        assert!(defaults.copy_fingerprint.is_none());
        assert!(defaults.provider_api_copy_fingerprint.is_none());
        assert!(defaults.adopt_existing_fingerprint.is_none());
        assert!(defaults.provider_object_id.is_none());
        assert!(defaults.oauth_connections.is_none());
        assert!(defaults.evidence_dir.is_none());
        assert!(defaults.evict_receipt.is_none());
        assert!(defaults.eviction_approval_dir.is_none());
        assert!(defaults.review_candidate_fingerprint.is_none());
        assert!(defaults.reviewed_by.is_none());
        assert!(defaults.review_rationale.is_none());
        assert!(defaults.export_naruon_lineage.is_none());
        assert!(!defaults.export_naruon_capacity);
        assert!(!defaults.export_naruon_copy_readiness);
        assert!(defaults.naruon_copy_readiness_output.is_none());
        assert!(!defaults.export_semantic_catalog);
        assert!(defaults.naruon_sync_evidence.is_none());
        assert!(!defaults.verify_capacity);
        assert!(!defaults.decision_summary);
        assert!(!defaults.all_readable_roots);
        assert!(defaults.review_reason_set.is_none());
        assert!(defaults.private_review_output.is_none());
        assert!(defaults.private_candidate_inspection_output.is_none());
        assert!(defaults.exact_duplicate_review_prefix.is_none());
        assert!(defaults.exact_duplicate_kind.is_none());
        assert!(!defaults.audit_receipts);
        assert_eq!(defaults.capacity_reserve_mib, 1024);
        let args = vec![
            "--root".into(),
            "/scan".into(),
            "--provider".into(),
            "icloud".into(),
            "--min-size-mib".into(),
            "1".into(),
            "--min-age-days".into(),
            "2".into(),
            "--limit".into(),
            "3".into(),
            "--decision-summary".into(),
            "--review-reason-set".into(),
            "metadata-review-required|download-origin-needs-destination-review".into(),
            "--private-review-output".into(),
            "/private-review.json".into(),
            "--verify-capacity".into(),
            "--capacity-reserve-mib".into(),
            "2048".into(),
        ];
        let parsed = parse_args(&args, Path::new("/home/test")).unwrap();
        assert_eq!(parsed.root, PathBuf::from("/scan"));
        assert_eq!(parsed.provider, Some(CloudProvider::Icloud));
        assert_eq!(
            (parsed.min_size_mib, parsed.min_age_days, parsed.limit),
            (1, 2, 3)
        );
        assert!(parsed.verify_capacity);
        assert!(parsed.decision_summary);
        assert_eq!(
            parsed.review_reason_set,
            Some(vec![
                "download-origin-needs-destination-review".into(),
                "metadata-review-required".into(),
            ])
        );
        assert_eq!(
            parsed.private_review_output,
            Some(PathBuf::from("/private-review.json"))
        );
        assert_eq!(parsed.capacity_reserve_mib, 2048);

        let inspection = parse_args(
            &[
                "--decision-summary".into(),
                "--private-candidate-inspection-output".into(),
                "/private-inspection.json".into(),
            ],
            Path::new("/home/test"),
        )
        .unwrap();
        assert_eq!(
            inspection.private_candidate_inspection_output,
            Some(PathBuf::from("/private-inspection.json"))
        );
        assert!(validate_action_args(&inspection).is_ok());

        let duplicate_review = parse_args(
            &[
                "--exact-duplicate-review-prefix".into(),
                "smart_bundle_".into(),
                "--exact-duplicate-kind".into(),
                "document".into(),
            ],
            Path::new("/home/test"),
        )
        .unwrap();
        assert_eq!(
            duplicate_review.exact_duplicate_review_prefix.as_deref(),
            Some("smart_bundle_")
        );
        assert_eq!(
            duplicate_review.exact_duplicate_kind,
            Some(ArchiveKind::Document)
        );
        assert!(validate_action_args(&duplicate_review).is_ok());
    }

    #[test]
    fn receipt_audit_requires_only_a_receipt_directory_and_is_read_only() {
        let audit = parse_args(
            &[
                "--audit-receipts".into(),
                "--receipt-dir".into(),
                "/app/cloud-receipts".into(),
            ],
            Path::new("/home/test"),
        )
        .unwrap();
        assert!(audit.audit_receipts);
        assert!(validate_action_args(&audit).is_ok());

        let audit_with_external_evidence = parse_args(
            &[
                "--audit-receipts".into(),
                "--receipt-dir".into(),
                "/receipts".into(),
                "--evidence-dir".into(),
                "/provider-evidence".into(),
            ],
            Path::new("/home/test"),
        )
        .unwrap();
        assert!(validate_action_args(&audit_with_external_evidence).is_ok());

        let missing_directory =
            parse_args(&["--audit-receipts".into()], Path::new("/home/test")).unwrap();
        assert!(validate_action_args(&missing_directory).is_err());
    }

    #[test]
    fn receipt_reconciliation_requires_local_evidence_and_is_distinct_from_audit() {
        let reconcile = parse_args(
            &[
                "--reconcile-receipts".into(),
                "--receipt-dir".into(),
                "/app/cloud-receipts".into(),
                "--evidence-dir".into(),
                "/app/cloud-provider-evidence".into(),
            ],
            Path::new("/home/test"),
        )
        .unwrap();
        assert!(reconcile.reconcile_receipts);
        assert!(!reconcile.audit_receipts);
        assert!(validate_action_args(&reconcile).is_ok());

        let missing_evidence = parse_args(
            &[
                "--reconcile-receipts".into(),
                "--receipt-dir".into(),
                "/receipts".into(),
            ],
            Path::new("/home/test"),
        )
        .unwrap();
        assert!(validate_action_args(&missing_evidence).is_err());
    }

    #[test]
    fn reconciliation_refreshes_projection_issues_after_writing_projections() {
        let mut entry = ReceiptReconciliationEntry {
            file_name: "receipt.json".into(),
            receipt_id: Some("receipt".into()),
            provider: Some(CloudProvider::Icloud),
            bytes: Some(1),
            source_state: Some("present".into()),
            destination_state: Some("present".into()),
            adr_projection_state: Some("valid".into()),
            goal_projection_state: Some("valid".into()),
            goal_status: Some("blocked".into()),
            goal_state: None,
            provider_sync_state: None,
            eviction_permit: false,
            attestation_error: None,
            evidence_record_count: 0,
            issues: vec![
                "adr-projection-invalid-schema".into(),
                "goal-projection-missing".into(),
                "provider-attestation-incomplete".into(),
            ],
        };

        refresh_projection_issues(&mut entry);
        assert_eq!(entry.issues, vec!["provider-attestation-incomplete"]);

        entry.goal_projection_state = Some("missing".into());
        refresh_projection_issues(&mut entry);
        assert_eq!(
            entry.issues,
            vec![
                "provider-attestation-incomplete",
                "goal-projection-missing"
            ]
        );
    }

    #[test]
    fn empty_receipt_reconciliation_does_not_claim_a_cloud_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let receipt_dir = temp.path().join("receipts");
        let evidence_dir = temp.path().join("evidence");
        std::fs::create_dir_all(&receipt_dir).unwrap();
        let report =
            reconcile_receipts(&receipt_dir, &evidence_dir, None, None, temp.path(), 10).unwrap();
        assert_eq!(report.attestation_attempted_count, 0);
        assert!(!report.mutation_performed);
        assert!(!report.cloud_write_executed);
        assert!(!report.source_eviction_authorized);
    }

    #[cfg(not(coverage))]
    #[test]
    fn headless_reconciliation_reports_receipts_left_after_entry_budget() {
        let temp = tempfile::tempdir().unwrap();
        let receipt_dir = temp.path().join("receipts");
        let evidence_dir = temp.path().join("evidence");
        std::fs::create_dir_all(&receipt_dir).unwrap();
        for index in 0..=MAX_RECONCILIATION_ATTESTATIONS {
            std::fs::write(receipt_dir.join(format!("{index:04}.json")), b"{}").unwrap();
        }

        let report =
            reconcile_receipts(&receipt_dir, &evidence_dir, None, None, temp.path(), 10).unwrap();
        assert_eq!(
            report.unprocessed_count,
            (receipt_dir.read_dir().unwrap().count() - MAX_RECONCILIATION_ATTESTATIONS) as u64
        );
        assert!(report.incomplete_reconciliation);
        assert!(report.notices.contains(&"reconciliation-entry-limit"));
    }

    #[test]
    fn receipt_audit_counts_legacy_evidence_without_double_counting() {
        let temp = tempfile::tempdir().unwrap();
        let receipt_dir = temp.path().join("cloud-receipts");
        let provider_dir = temp.path().join("cloud-provider-evidence");
        let legacy_dir = temp.path().join("cloud-sync-evidence");
        std::fs::create_dir_all(&receipt_dir).unwrap();
        std::fs::create_dir_all(&provider_dir).unwrap();
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::write(provider_dir.join("abc-1.json"), b"provider").unwrap();
        std::fs::write(legacy_dir.join("abc-1.json"), b"legacy-copy").unwrap();
        std::fs::write(legacy_dir.join("abc-2.json"), b"legacy").unwrap();

        let dirs = audit_evidence_dirs(&receipt_dir, None);
        assert_eq!(evidence_record_count(&dirs, "abc"), 2);
        assert_eq!(
            audit_evidence_dirs(&receipt_dir, Some(&legacy_dir)),
            vec![legacy_dir]
        );
    }

    #[test]
    fn receipt_audit_rejects_unbound_or_outdated_projections() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("projection.json");
        let receipt_id = "a".repeat(64);
        let snapshot = cloud_adr::CloudOffloadAdrSnapshot {
            schema_version: cloud_adr::CLOUD_ADR_SCHEMA_VERSION,
            adr_id: format!("cloud-offload:{receipt_id}"),
            receipt_id: receipt_id.clone(),
            goal_state: cloud_transfer::CloudOffloadGoalState::CopyVerified,
            provider_sync_state: cloud_transfer::ProviderSyncState::Unknown,
            sync_complete: false,
            context: vec![
                "metadata-first-lineage".into(),
                "goal-state:copy-verified".into(),
                "provider-sync-state:unknown".into(),
                "provider-sync-complete:false".into(),
                "filename-dates-auxiliary".into(),
                "production-time-precedence:embedded-metadata>explicit-filename-date>filesystem-created>filesystem-modified".into(),
                "provider-evidence-authoritative".into(),
                "source-retained-until-explicit-trash-step".into(),
            ],
            decision: "retain-source-after-copy".into(),
            consequences: vec!["source-retained".into()],
            evidence_record_id: None,
            updated_at_ms: 1,
        };
        std::fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        assert_eq!(projection_state(&path, "adr", &receipt_id), "valid");

        let mut unbound = snapshot.clone();
        unbound.receipt_id = "b".repeat(64);
        std::fs::write(&path, serde_json::to_vec(&unbound).unwrap()).unwrap();
        assert_eq!(
            projection_state(&path, "adr", &receipt_id),
            "invalid-binding"
        );

        let mut outdated = snapshot;
        outdated.schema_version = 1;
        std::fs::write(&path, serde_json::to_vec(&outdated).unwrap()).unwrap();
        assert_eq!(
            projection_state(&path, "adr", &receipt_id),
            "invalid-schema"
        );
    }

    #[test]
    fn all_readable_roots_is_dry_run_summary_with_optional_capacity() {
        let valid = parse_args(
            &["--all-readable-roots".into(), "--decision-summary".into()],
            Path::new("/h"),
        )
        .unwrap();
        assert!(valid.all_readable_roots);
        assert!(validate_action_args(&valid).is_ok());

        let missing_summary =
            parse_args(&["--all-readable-roots".into()], Path::new("/h")).unwrap();
        assert!(validate_action_args(&missing_summary).is_err());
        let scoped = parse_args(
            &[
                "--all-readable-roots".into(),
                "--decision-summary".into(),
                "--provider".into(),
                "icloud".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&scoped).is_err());
        let capacity = parse_args(
            &[
                "--all-readable-roots".into(),
                "--decision-summary".into(),
                "--verify-capacity".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(capacity.verify_capacity);
        assert!(validate_action_args(&capacity).is_ok());
    }

    #[test]
    fn parser_and_selector_reject_ambiguous_or_invalid_input() {
        assert!(parse_args(&["--wat".into()], Path::new("/h")).is_err());
        assert!(parse_args(&["--provider".into(), "box".into()], Path::new("/h")).is_err());
        assert!(parse_args(&["--limit".into(), "x".into()], Path::new("/h")).is_err());
        assert!(parse_args(
            &["--capacity-reserve-mib".into(), "x".into()],
            Path::new("/h")
        )
        .is_err());
        assert!(parse_args(&["--root".into()], Path::new("/h")).is_err());
        for reason_set in [
            "",
            "duplicate|duplicate",
            "Uppercase-not-allowed",
            "contains_space",
            "destination-account-scope-unknown|",
        ] {
            assert!(parse_args(
                &[
                    "--decision-summary".into(),
                    "--review-reason-set".into(),
                    reason_set.into(),
                ],
                Path::new("/h"),
            )
            .is_err());
        }
        assert!(parse_args(
            &[
                "--review-reason-set".into(),
                "first-reason".into(),
                "--review-reason-set".into(),
                "second-reason".into(),
            ],
            Path::new("/h"),
        )
        .is_err());
        let inspect = parse_args(&["--inspect-roots".into()], Path::new("/h")).unwrap();
        assert!(inspect.inspect_roots);
        let both = parse_args(
            &["--list-roots".into(), "--inspect-roots".into()],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&both).is_err());
        let summary_action = parse_args(
            &["--decision-summary".into(), "--list-roots".into()],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&summary_action).is_err());
        let reason_set_without_summary = parse_args(
            &[
                "--review-reason-set".into(),
                "destination-account-scope-unknown".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&reason_set_without_summary).is_err());
        let reason_set_summary = parse_args(
            &[
                "--decision-summary".into(),
                "--review-reason-set".into(),
                "destination-account-scope-unknown".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&reason_set_summary).is_ok());
        let private_review = parse_args(
            &[
                "--decision-summary".into(),
                "--review-reason-set".into(),
                "destination-account-scope-unknown".into(),
                "--private-review-output".into(),
                "/private/review.json".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&private_review).is_ok());
        let mut relative_private_review = private_review.clone();
        relative_private_review.private_review_output = Some(PathBuf::from("review.json"));
        assert!(validate_action_args(&relative_private_review).is_err());
        let mut missing_reason_set = private_review.clone();
        missing_reason_set.review_reason_set = None;
        assert!(validate_action_args(&missing_reason_set).is_err());
        let mut multicloud_private_review = private_review;
        multicloud_private_review.all_readable_roots = true;
        assert!(validate_action_args(&multicloud_private_review).is_err());
        let private_inspection = parse_args(
            &[
                "--decision-summary".into(),
                "--private-candidate-inspection-output".into(),
                "/private/inspection.json".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&private_inspection).is_ok());
        let mut missing_inspection_summary = private_inspection.clone();
        missing_inspection_summary.decision_summary = false;
        assert!(validate_action_args(&missing_inspection_summary).is_err());
        let mut relative_private_inspection = private_inspection.clone();
        relative_private_inspection.private_candidate_inspection_output =
            Some(PathBuf::from("inspection.json"));
        assert!(validate_action_args(&relative_private_inspection).is_err());
        let mut exact_subset_conflict = private_inspection.clone();
        exact_subset_conflict.review_reason_set =
            Some(vec!["destination-account-scope-unknown".into()]);
        assert!(validate_action_args(&exact_subset_conflict).is_err());
        let mut multicloud_private_inspection = private_inspection;
        multicloud_private_inspection.all_readable_roots = true;
        assert!(validate_action_args(&multicloud_private_inspection).is_err());
        for prefix in ["", ".", "..", "nested/path", "nested\\path", "line\nbreak"] {
            assert!(parse_args(
                &[
                    "--exact-duplicate-review-prefix".into(),
                    prefix.into(),
                    "--exact-duplicate-kind".into(),
                    "document".into(),
                ],
                Path::new("/h"),
            )
            .is_err());
        }
        assert!(parse_args(
            &[
                "--exact-duplicate-review-prefix".into(),
                "bundle_".into(),
                "--exact-duplicate-kind".into(),
                "unknown".into(),
            ],
            Path::new("/h"),
        )
        .is_err());
        let missing_duplicate_kind = parse_args(
            &["--exact-duplicate-review-prefix".into(), "bundle_".into()],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&missing_duplicate_kind).is_err());
        let duplicate_summary_conflict = parse_args(
            &[
                "--exact-duplicate-review-prefix".into(),
                "bundle_".into(),
                "--exact-duplicate-kind".into(),
                "document".into(),
                "--decision-summary".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&duplicate_summary_conflict).is_err());
        let roots = vec![
            CloudRoot {
                id: "/a".into(),
                provider: CloudProvider::Icloud,
                account_scope: disksage_lib::cloud::CloudAccountScope::Unknown,
                label: "a".into(),
                path: "/a".into(),
                readable: true,
                access_issue: None,
            },
            CloudRoot {
                id: "/b".into(),
                provider: CloudProvider::Icloud,
                account_scope: disksage_lib::cloud::CloudAccountScope::Unknown,
                label: "b".into(),
                path: "/b".into(),
                readable: true,
                access_issue: None,
            },
        ];
        let mut args = parse_args(&[], Path::new("/h")).unwrap();
        assert!(select_root(&roots, &args).is_err());
        args.cloud_root = Some(PathBuf::from("/b"));
        assert_eq!(select_root(&roots, &args).unwrap().path, "/b");
        args.cloud_root = Some(PathBuf::from("/missing"));
        assert!(select_root(&roots, &args).is_err());
    }

    #[test]
    fn selector_accepts_canonically_equivalent_unicode_path_and_fails_ambiguous() {
        let decomposed = "/cloud/GoogleDrive-user/\u{1102}\u{1162} \u{1103}\u{1173}\u{1105}\u{1161}\u{110b}\u{1175}\u{1107}\u{1173}";
        let composed = "/cloud/GoogleDrive-user/내 드라이브";
        let root = CloudRoot {
            id: decomposed.into(),
            provider: CloudProvider::GoogleDrive,
            account_scope: disksage_lib::cloud::CloudAccountScope::Personal,
            label: "Google Drive".into(),
            path: decomposed.into(),
            readable: true,
            access_issue: None,
        };
        let mut args = parse_args(&[], Path::new("/home/test")).unwrap();
        args.cloud_root = Some(PathBuf::from(composed));

        assert_eq!(select_root(&[root.clone()], &args).unwrap(), root);

        let canonically_equivalent_duplicate = CloudRoot {
            id: composed.into(),
            path: composed.into(),
            ..root.clone()
        };
        assert!(select_root(&[root, canonically_equivalent_duplicate], &args).is_err());
    }

    #[test]
    fn decision_summary_keeps_review_evidence_and_redacts_paths_and_sensitive_values() {
        let candidate = cloud::CloudCandidate {
            metadata_fingerprint: "a".repeat(64),
            review_fingerprint: "b".repeat(64),
            src: "/Users/private/Downloads/report.pdf".into(),
            dst: "/Users/private/Cloud/report.pdf".into(),
            provider: CloudProvider::Icloud,
            destination_account_scope: disksage_lib::cloud::CloudAccountScope::Personal,
            kind: cloud::ArchiveKind::Document,
            bytes: 42,
            age_days: 7,
            created_ms: 10,
            modified_ms: 20,
            production_time_ms: 5,
            production_time_source: "embedded-pdf-creation-date".into(),
            production_time_confidence: "high".into(),
            source_root: "/Users/private/Downloads".into(),
            relative_path: "report.pdf".into(),
            source_context: "Downloads".into(),
            requires_review: true,
            review_reasons: vec![
                "download-origin-needs-destination-review".into(),
                "metadata-review-required".into(),
            ],
            content_title: Some("Confidential title".into()),
            content_authors: vec!["Private Author".into()],
            content_context: vec!["private context".into()],
            duration_ms: None,
            dataset_profile: None,
            metadata_evidence: vec![cloud::MetadataEvidence {
                field: "creation-date".into(),
                value: "private raw value".into(),
                source: "pdf-info".into(),
                confidence: "high".into(),
            }],
            blocked_reason: None,
        };
        let mut report = cloud::CloudPlanReport {
            cloud_root: CloudRoot {
                id: "/Users/private/Cloud".into(),
                provider: CloudProvider::Icloud,
                account_scope: disksage_lib::cloud::CloudAccountScope::Personal,
                label: "private@example.com".into(),
                path: "/Users/private/Cloud".into(),
                readable: true,
                access_issue: None,
            },
            generated_at_ms: 100,
            source_selection_policy: Some(cloud::CloudPlanOptions {
                min_size_bytes: 90 * 1024 * 1024,
                min_age_days: 30,
                limit: 200,
            }),
            candidates: vec![candidate],
            candidate_bytes: 42,
            potentially_reclaimable_bytes: 42,
            exact_duplicates: cloud::ExactDuplicateSummary::default(),
            capacity: None,
            local_volume: None,
            pre_copy_evidence: None,
            notices: vec!["dry-run-only".into()],
        };

        let summary = decision_summary(&report);
        let item = &summary["decisions"][0];
        assert_eq!(summary["output_mode"], "decision-summary");
        assert_eq!(summary["schema_version"], 3);
        assert!(summary["redacted_from_summary"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("relative-source-path-and-file-name")));
        assert_eq!(summary["candidate_count"], 1);
        assert_eq!(
            summary["source_selection_policy"]["min_size_bytes"],
            90 * 1024 * 1024
        );
        assert_eq!(summary["source_selection_policy"]["min_age_days"], 30);
        assert_eq!(summary["source_selection_policy"]["limit"], 200);
        assert_eq!(
            summary["decision_batch_fingerprint_version"],
            cloud::CLOUD_DECISION_BATCH_FINGERPRINT_VERSION
        );
        assert_eq!(
            summary["decision_batch_fingerprint"]
                .as_str()
                .unwrap()
                .len(),
            64
        );
        assert!(item.get("relative_path").is_none());
        assert_eq!(item["decision_state"], "review-required");
        assert_eq!(item["copy_approval_action"], "copy-only");
        assert_eq!(
            item["exact_copy_approval_phrase"],
            format!(
                "DiskSage cloud copy-only {} 승인",
                item["review_fingerprint"].as_str().unwrap()
            )
        );
        assert_eq!(item["copy_approval_max_age_ms"], 15 * 60 * 1000);
        let mut destination_exists = report.candidates[0].clone();
        destination_exists.blocked_reason = Some("destination-exists".into());
        let adoption = redacted_decision(&destination_exists);
        assert_eq!(adoption["copy_approval_action"], "adopt-existing-copy");
        assert_eq!(
            adoption["exact_copy_approval_phrase"],
            format!(
                "DiskSage cloud adopt-existing-copy {} 승인",
                adoption["review_fingerprint"].as_str().unwrap()
            )
        );

        destination_exists.blocked_reason = Some("incomplete-download".into());
        let ineligible = redacted_decision(&destination_exists);
        assert!(ineligible["copy_approval_action"].is_null());
        assert!(ineligible["exact_copy_approval_phrase"].is_null());
        assert_eq!(
            summary["aggregates"]["decision_state"]["counts"]["review-required"],
            1
        );
        assert_eq!(
            summary["aggregates"]["decision_state"]["candidate_bytes"]["review-required"],
            42
        );
        assert_eq!(
            summary["aggregates"]["review_required_reason"]["counts"]["metadata-review-required"],
            1
        );
        assert_eq!(
            summary["aggregates"]["review_required_reason"]["candidate_bytes"]
                ["download-origin-needs-destination-review"],
            42
        );
        assert_eq!(
            summary["aggregates"]["review_required_reason"]
                ["candidate_bytes_can_overlap_across_reasons"],
            true
        );
        assert!(
            summary["aggregates"]["review_required_reason"]["sole_reason_counts"]
                ["metadata-review-required"]
                .is_null()
        );
        assert_eq!(
            summary["aggregates"]["review_required_reason"]["reason_count_distribution"]["2"],
            1
        );
        assert_eq!(
            summary["aggregates"]["review_required_reason"]["reason_set_counts"]
                ["download-origin-needs-destination-review|metadata-review-required"],
            1
        );
        assert_eq!(
            summary["aggregates"]["review_required_reason"]["reason_set_delimiter"],
            "|"
        );
        assert_eq!(
            summary["aggregates"]["production_time_source"]["counts"]["embedded-pdf-creation-date"],
            1
        );
        assert_eq!(
            summary["aggregates"]["production_time_confidence"]["counts"]["high"],
            1
        );
        assert!(item.get("src").is_none());
        assert!(item.get("dst").is_none());
        assert!(item.get("metadata_evidence").is_none());

        let encoded = serde_json::to_string(&summary).unwrap();
        for redacted in [
            "/Users/private",
            "private@example.com",
            "Confidential title",
            "Private Author",
            "private raw value",
            "report.pdf",
        ] {
            assert!(!encoded.contains(redacted));
        }

        let mut compact_report = report.clone();
        compact_report.exact_duplicates = cloud::ExactDuplicateSummary {
            cluster_count: 1,
            candidate_count: 2,
            candidate_bytes: 84,
            redundant_bytes: 42,
            clusters: vec![cloud::ExactDuplicateClusterRecommendation {
                cluster_fingerprint: "c".repeat(64),
                candidate_count: 2,
                bytes_per_candidate: 42,
                redundant_bytes: 42,
                recommended_canonical_metadata_fingerprint: "d".repeat(64),
                recommendation_confidence: "high".into(),
                recommendation_reason_codes: vec!["richer-source-lineage-context-preferred".into()],
                member_metadata_fingerprints: vec!["d".repeat(64), "e".repeat(64)],
                requires_human_confirmation: true,
            }],
        };
        let compact = compact_decision_summary(&compact_report);
        assert_eq!(compact["output_mode"], "compact-decision-summary");
        assert_eq!(compact["schema_version"], 3);
        assert_eq!(compact["candidate_details_included"], false);
        assert_eq!(compact["cloud_write_executed"], false);
        assert_eq!(compact["source_eviction_authorized"], false);
        assert_eq!(
            compact["metadata_policy"]["exact_human_attributed_copy_approval_required"],
            true
        );
        assert_eq!(compact["exact_duplicates"]["cluster_count"], 1);
        assert_eq!(compact["exact_duplicates"]["redundant_bytes"], 42);
        assert_eq!(compact["exact_duplicates"]["cluster_members_omitted"], true);
        assert_eq!(
            compact["organization_manifest"]["candidate_groups"]["archive_kind"]["counts"]
                ["document"],
            1
        );
        assert_eq!(
            compact["organization_manifest"]["candidate_groups"]["production_month"]["counts"]
                ["1970-01"],
            1
        );
        assert_eq!(
            compact["organization_manifest"]["context_evidence_coverage"]["counts"]
                ["content-context-present"],
            1
        );
        assert_eq!(
            compact["organization_manifest"]["context_evidence_coverage"]["counts"]
                ["nested-source-context-preserved"],
            1
        );
        assert_eq!(
            compact["organization_manifest"]["destination_collision_preflight"]["counts"],
            0
        );
        assert_eq!(
            compact["organization_manifest"]["source_relative_path_preserved_for_lineage"],
            true
        );
        assert!(compact.get("decisions").is_none());
        assert!(compact["exact_duplicates"].get("clusters").is_none());
        assert!(compact["aggregates"]["review_required_reason"]
            .get("reason_set_counts")
            .is_none());
        assert_eq!(
            compact["aggregates"]["review_required_reason"]["counts"]["metadata-review-required"],
            1
        );
        assert_eq!(
            compact["decision_batch_fingerprint"],
            cloud::cloud_decision_batch_fingerprint(&compact_report)
        );
        let encoded_compact = serde_json::to_string(&compact).unwrap();
        for redacted in [
            "/Users/private".to_string(),
            "private@example.com".to_string(),
            "Confidential title".to_string(),
            "Private Author".to_string(),
            "private raw value".to_string(),
            "report.pdf".to_string(),
            "c".repeat(64),
            "d".repeat(64),
            "e".repeat(64),
        ] {
            assert!(!encoded_compact.contains(&redacted));
        }

        let reason_set = report.candidates[0].review_reasons.clone();
        let review_batch = review_batch_summary(&report, &reason_set).unwrap();
        assert_eq!(review_batch["output_mode"], "review-batch-summary");
        assert_eq!(review_batch["schema_version"], 3);
        assert!(review_batch["redacted_from_summary"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("relative-source-path-and-file-name")));
        assert_eq!(review_batch["candidate_count"], 1);
        assert_eq!(review_batch["candidate_bytes"], 42);
        assert_eq!(review_batch["reason_set"], serde_json::json!(reason_set));
        assert!(review_batch["decisions"][0].get("relative_path").is_none());
        assert!(!serde_json::to_string(&review_batch)
            .unwrap()
            .contains("report.pdf"));
        assert_eq!(
            review_batch["review_batch_fingerprint"]
                .as_str()
                .unwrap()
                .len(),
            64
        );
        assert_eq!(
            review_batch["metadata_policy"]["batch_fingerprint_is_not_approval"],
            true
        );
        assert_eq!(
            review_batch["metadata_policy"]["candidate_review_decisions_remain_individual"],
            true
        );
        assert_eq!(
            review_batch_summary(&report, &report.candidates[0].review_reasons).unwrap()
                ["review_batch_fingerprint"],
            review_batch["review_batch_fingerprint"]
        );
        let mut unrelated_changed = report.clone();
        let mut unrelated = unrelated_changed.candidates[0].clone();
        unrelated.metadata_fingerprint = "c".repeat(64);
        unrelated.review_fingerprint = "d".repeat(64);
        unrelated.relative_path = "other.pdf".into();
        unrelated.src = "/Users/private/Downloads/other.pdf".into();
        unrelated.dst = "/Users/private/Cloud/other.pdf".into();
        unrelated.bytes = 9;
        unrelated.review_reasons = vec!["different-reason".into()];
        unrelated_changed.candidates.push(unrelated);
        unrelated_changed.candidate_bytes += 9;
        unrelated_changed.potentially_reclaimable_bytes += 9;
        let unrelated_batch = review_batch_summary(&unrelated_changed, &reason_set).unwrap();
        assert_ne!(
            unrelated_batch["decision_batch_fingerprint"],
            review_batch["decision_batch_fingerprint"]
        );
        assert_eq!(
            unrelated_batch["review_batch_fingerprint"],
            review_batch["review_batch_fingerprint"]
        );

        let mut selected_changed = report.clone();
        selected_changed.candidates[0].review_fingerprint = "e".repeat(64);
        assert_ne!(
            review_batch_summary(&selected_changed, &reason_set).unwrap()
                ["review_batch_fingerprint"],
            review_batch["review_batch_fingerprint"]
        );
        let encoded_batch = serde_json::to_string(&review_batch).unwrap();
        for redacted in [
            "/Users/private",
            "private@example.com",
            "Confidential title",
            "Private Author",
            "private raw value",
        ] {
            assert!(!encoded_batch.contains(redacted));
        }
        assert!(review_batch_summary(&report, &["not-present".into()]).is_err());

        let dossier = private_review_dossier(&report, &reason_set).unwrap();
        assert_eq!(dossier["output_mode"], "private-review-dossier");
        assert_eq!(
            dossier["review_batch_fingerprint"],
            review_batch["review_batch_fingerprint"]
        );
        assert_eq!(dossier["candidate_count"], 1);
        assert_eq!(dossier["candidate_bytes"], 42);
        assert_eq!(
            dossier["metadata_policy"]["filename_dates_are_auxiliary"],
            true
        );
        assert_eq!(
            dossier["metadata_policy"]["candidate_review_decisions_remain_individual"],
            true
        );
        let encoded_dossier = serde_json::to_string(&dossier).unwrap();
        for private_value in [
            "/Users/private",
            "private@example.com",
            "Confidential title",
            "Private Author",
            "private raw value",
        ] {
            assert!(encoded_dossier.contains(private_value));
        }

        let mut inspection_report = report.clone();
        let mut blocked = inspection_report.candidates[0].clone();
        blocked.metadata_fingerprint = "1".repeat(64);
        blocked.review_fingerprint = "2".repeat(64);
        blocked.relative_path = "blocked-private.zip".into();
        blocked.src = "/Users/private/Downloads/blocked-private.zip".into();
        blocked.dst = "/Users/private/Cloud/blocked-private.zip".into();
        blocked.blocked_reason = Some("opaque-container-content-uninspected".into());
        inspection_report.candidates.push(blocked);
        inspection_report.candidate_bytes = 84;
        let inspection = private_candidate_inspection_dossier(&inspection_report);
        assert_eq!(
            inspection["output_mode"],
            "private-candidate-inspection-dossier"
        );
        assert_eq!(
            inspection["inspection_scope"],
            "all-current-plan-candidates"
        );
        assert_eq!(inspection["candidate_count"], 2);
        assert_eq!(inspection["candidate_bytes"], 84);
        assert_eq!(inspection["decision_state"]["counts"]["blocked"], 1);
        assert_eq!(inspection["decision_state"]["counts"]["review-required"], 1);
        assert_eq!(
            inspection["metadata_policy"]["inspection_includes_blocked_candidates"],
            true
        );
        assert_eq!(
            inspection["metadata_policy"]["dossier_is_not_approval"],
            true
        );
        assert_eq!(inspection["cloud_write_executed"], false);
        assert_eq!(inspection["source_eviction_authorized"], false);
        assert!(inspection["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|candidate| {
                candidate["decision_state"] == "blocked"
                    && candidate["relative_path"] == "blocked-private.zip"
            }));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let private_dir = tempfile::tempdir().unwrap();
            let private_path = private_dir.path().join("review.json");
            let (sha256, bytes) = write_private_review_dossier(&private_path, &dossier).unwrap();
            assert_eq!(sha256.len(), 64);
            assert!(bytes > 0);
            assert_eq!(std::fs::read(&private_path).unwrap().len(), bytes);
            assert!(write_private_review_dossier(&private_path, &dossier).is_err());
            assert_eq!(
                std::fs::metadata(&private_path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

        let mut mixed = report.clone();
        let mut ready = mixed.candidates[0].clone();
        ready.metadata_fingerprint = "c".repeat(64);
        ready.review_fingerprint = "d".repeat(64);
        ready.bytes = 10;
        ready.requires_review = false;
        ready.review_reasons.clear();
        ready.production_time_source = "filename:path-token".into();
        ready.production_time_confidence = "low".into();
        let mut blocked = mixed.candidates[0].clone();
        blocked.metadata_fingerprint = "e".repeat(64);
        blocked.review_fingerprint = "f".repeat(64);
        blocked.bytes = 7;
        blocked.blocked_reason = Some("incomplete-download".into());
        mixed.candidates.extend([ready, blocked]);
        let aggregates = decision_aggregates(&mixed);
        assert_eq!(aggregates["decision_state"]["counts"]["review-required"], 1);
        assert_eq!(
            aggregates["decision_state"]["counts"]["ready-for-copy-review"],
            1
        );
        assert_eq!(aggregates["decision_state"]["counts"]["blocked"], 1);
        assert_eq!(
            aggregates["blocked_reason"]["counts"]["incomplete-download"],
            1
        );
        assert_eq!(
            aggregates["review_required_reason"]["counts"]["metadata-review-required"],
            1
        );
        assert_eq!(
            aggregates["production_time_source"]["counts"]["filename:path-token"],
            1
        );

        let mut sole_reason = report.clone();
        sole_reason.candidates[0].review_reasons = vec!["destination-account-scope-unknown".into()];
        let sole_reason_aggregates = decision_aggregates(&sole_reason);
        assert_eq!(
            sole_reason_aggregates["review_required_reason"]["sole_reason_counts"]
                ["destination-account-scope-unknown"],
            1
        );
        assert_eq!(
            sole_reason_aggregates["review_required_reason"]["sole_reason_candidate_bytes"]
                ["destination-account-scope-unknown"],
            42
        );

        let original_batch = cloud::cloud_decision_batch_fingerprint(&report);
        let mut volatile_changed = report.clone();
        volatile_changed.generated_at_ms += 1;
        volatile_changed
            .notices
            .push("fresh-capacity-required".into());
        assert_eq!(
            cloud::cloud_decision_batch_fingerprint(&volatile_changed),
            original_batch
        );
        assert_eq!(
            review_batch_summary(&volatile_changed, &reason_set).unwrap()
                ["review_batch_fingerprint"],
            review_batch["review_batch_fingerprint"]
        );

        let mut evidence_changed = report.clone();
        evidence_changed.candidates[0].review_fingerprint = "c".repeat(64);
        assert_ne!(
            cloud::cloud_decision_batch_fingerprint(&evidence_changed),
            original_batch
        );

        let mut selection_changed = report.clone();
        selection_changed
            .source_selection_policy
            .as_mut()
            .unwrap()
            .min_size_bytes += 1;
        assert_ne!(
            cloud::cloud_decision_batch_fingerprint(&selection_changed),
            original_batch
        );

        let mut blocker_changed = report.clone();
        blocker_changed.candidates[0].blocked_reason = Some("destination-exists".into());
        blocker_changed.potentially_reclaimable_bytes = 0;
        assert_ne!(
            cloud::cloud_decision_batch_fingerprint(&blocker_changed),
            original_batch
        );

        let mut reordered = report.clone();
        let mut second = reordered.candidates[0].clone();
        second.metadata_fingerprint = "d".repeat(64);
        second.review_fingerprint = "e".repeat(64);
        reordered.candidates.push(second);
        reordered.candidate_bytes *= 2;
        reordered.potentially_reclaimable_bytes *= 2;
        let ordered_batch = cloud::cloud_decision_batch_fingerprint(&reordered);
        reordered.candidates.reverse();
        assert_eq!(
            cloud::cloud_decision_batch_fingerprint(&reordered),
            ordered_batch
        );

        report.candidates[0].requires_review = false;
        assert_eq!(
            candidate_decision_state(&report.candidates[0]),
            "ready-for-copy-review"
        );
        report.candidates[0].blocked_reason = Some("incomplete-download".into());
        assert_eq!(candidate_decision_state(&report.candidates[0]), "blocked");
    }

    #[test]
    fn exact_duplicate_review_batch_binds_only_root_canonical_and_nested_prefix_copies() {
        let content_sha256 = "1".repeat(64);
        let member = |metadata_fingerprint: &str,
                      review_fingerprint: &str,
                      relative_path: &str,
                      context: Vec<String>| {
            cloud::CloudCandidate {
                metadata_fingerprint: metadata_fingerprint.repeat(64),
                review_fingerprint: review_fingerprint.repeat(64),
                src: format!("/Users/private/Downloads/{relative_path}"),
                dst: format!("/Users/private/Cloud/{relative_path}"),
                provider: CloudProvider::Icloud,
                destination_account_scope: disksage_lib::cloud::CloudAccountScope::Personal,
                kind: ArchiveKind::Document,
                bytes: 42,
                age_days: 7,
                created_ms: 10,
                modified_ms: 20,
                production_time_ms: 30,
                production_time_source: "embedded:exiftool:CreateDate".into(),
                production_time_confidence: "high".into(),
                source_root: "/Users/private/Downloads".into(),
                relative_path: relative_path.into(),
                source_context: "private-source-context".into(),
                requires_review: true,
                review_reasons: vec![
                    "download-origin-needs-destination-review".into(),
                    "exact-duplicate-content-needs-canonical-selection".into(),
                ],
                content_title: Some("Private title".into()),
                content_authors: vec!["Private author".into()],
                content_context: context,
                duration_ms: None,
                dataset_profile: None,
                metadata_evidence: vec![
                    cloud::MetadataEvidence {
                        field: "production-date".into(),
                        value: "private-production-value".into(),
                        source: "embedded:exiftool:CreateDate".into(),
                        confidence: "high".into(),
                    },
                    cloud::MetadataEvidence {
                        field: "exact-duplicate-content-sha256".into(),
                        value: content_sha256.clone(),
                        source: "local:content-hash".into(),
                        confidence: "high".into(),
                    },
                ],
                blocked_reason: None,
            }
        };
        let canonical = member(
            "a",
            "b",
            "report.docx",
            vec![
                "download-origin-host=private.example".into(),
                "download-agent=Edge".into(),
            ],
        );
        let redundant = member(
            "c",
            "d",
            "smart_bundle_v1/report.docx",
            vec!["download-agent=Bandizip".into()],
        );
        let report = cloud::CloudPlanReport {
            cloud_root: CloudRoot {
                id: "/Users/private/Cloud".into(),
                provider: CloudProvider::Icloud,
                account_scope: disksage_lib::cloud::CloudAccountScope::Personal,
                label: "private@example.com".into(),
                path: "/Users/private/Cloud".into(),
                readable: true,
                access_issue: None,
            },
            generated_at_ms: 100,
            source_selection_policy: Some(cloud::CloudPlanOptions::default()),
            candidates: vec![canonical, redundant],
            candidate_bytes: 84,
            potentially_reclaimable_bytes: 84,
            exact_duplicates: cloud::ExactDuplicateSummary {
                cluster_count: 1,
                candidate_count: 2,
                candidate_bytes: 84,
                redundant_bytes: 42,
                clusters: vec![cloud::ExactDuplicateClusterRecommendation {
                    cluster_fingerprint: "e".repeat(64),
                    candidate_count: 2,
                    bytes_per_candidate: 42,
                    redundant_bytes: 42,
                    recommended_canonical_metadata_fingerprint: "a".repeat(64),
                    recommendation_confidence: "high".into(),
                    recommendation_reason_codes: vec![
                        "richer-source-lineage-context-preferred".into()
                    ],
                    member_metadata_fingerprints: vec!["a".repeat(64), "c".repeat(64)],
                    requires_human_confirmation: true,
                }],
            },
            capacity: None,
            local_volume: None,
            pre_copy_evidence: None,
            notices: vec!["dry-run-only".into()],
        };

        let batch =
            exact_duplicate_review_batch(&report, "smart_bundle_", ArchiveKind::Document).unwrap();
        assert_eq!(batch["output_mode"], "exact-duplicate-review-batch");
        assert_eq!(batch["cluster_count"], 1);
        assert_eq!(batch["candidate_count"], 2);
        assert_eq!(batch["redundant_copy_count"], 1);
        assert_eq!(batch["redundant_bytes"], 42);
        assert_eq!(batch["clusters"][0]["content_sha256"], content_sha256);
        assert_eq!(
            batch["clusters"][0]["canonical"]["relative_path"],
            "report.docx"
        );
        assert_eq!(
            batch["clusters"][0]["redundant_copies"][0]["relative_path"],
            "smart_bundle_v1/report.docx"
        );
        assert_eq!(
            batch["clusters"][0]["canonical"]["source_lineage_evidence_fields"],
            serde_json::json!(["download-agent", "download-origin-host"])
        );
        assert_eq!(
            batch["exact_duplicate_review_batch_fingerprint"]
                .as_str()
                .unwrap()
                .len(),
            64
        );
        assert_eq!(
            batch["metadata_policy"]["batch_fingerprint_is_not_approval"],
            true
        );
        assert_eq!(
            batch["metadata_policy"]["trash_execution_is_not_available_in_this_output_mode"],
            true
        );

        let encoded = serde_json::to_string(&batch).unwrap();
        for redacted in [
            "/Users/private",
            "private@example.com",
            "private.example",
            "Private title",
            "Private author",
            "private-production-value",
            "private-source-context",
            "Bandizip",
            "Edge",
        ] {
            assert!(!encoded.contains(redacted));
        }

        let repeated =
            exact_duplicate_review_batch(&report, "smart_bundle_", ArchiveKind::Document).unwrap();
        assert_eq!(
            repeated["exact_duplicate_review_batch_fingerprint"],
            batch["exact_duplicate_review_batch_fingerprint"]
        );
        assert!(exact_duplicate_review_batch(&report, "other_", ArchiveKind::Document).is_err());
        assert!(
            exact_duplicate_review_batch(&report, "smart_bundle_", ArchiveKind::Media).is_err()
        );

        let mut evidence_changed = report.clone();
        evidence_changed.candidates[1].review_fingerprint = "f".repeat(64);
        assert_ne!(
            exact_duplicate_review_batch(
                &evidence_changed,
                "smart_bundle_",
                ArchiveKind::Document,
            )
            .unwrap()["exact_duplicate_review_batch_fingerprint"],
            batch["exact_duplicate_review_batch_fingerprint"]
        );

        let mut incomplete = report;
        incomplete.candidates.pop();
        assert!(
            exact_duplicate_review_batch(&incomplete, "smart_bundle_", ArchiveKind::Document)
                .unwrap_err()
                .contains("missing-from-bounded-plan")
        );
    }

    #[test]
    fn action_validation_requires_explicit_consistent_copy_arguments() {
        let mut args = parse_args(&[], Path::new("/h")).unwrap();
        args.copy_fingerprint = Some("a".repeat(64));
        assert!(validate_action_args(&args).is_err());
        args.receipt_dir = Some(PathBuf::from("/receipts"));
        args.confirm_copy_phrase = Some("exact copy phrase".into());
        args.reviewed_by = Some("human:local:test".into());
        args.review_rationale = Some("exact copy reviewed".into());
        assert!(validate_action_args(&args).is_ok());
        args.receipt_dir = Some(PathBuf::from("relative-receipts"));
        assert!(validate_action_args(&args).is_err());
        args.receipt_dir = Some(PathBuf::from("/receipts"));
        args.copy_fingerprint = Some("not-a-fingerprint".into());
        assert!(validate_action_args(&args).is_err());

        args.list_roots = false;
        args.attest_receipt = Some(PathBuf::from("relative-receipt.json"));
        assert!(validate_action_args(&args).is_err());
        args.copy_fingerprint = None;
        args.receipt_dir = None;
        args.list_roots = true;
        args.attest_receipt = Some(PathBuf::from("/receipt.json"));
        assert!(validate_action_args(&args).is_err());

        let parsed = parse_args(
            &[
                "--copy-fingerprint".into(),
                "b".repeat(64),
                "--receipt-dir".into(),
                "/receipts".into(),
                "--confirm-copy-phrase".into(),
                "exact copy phrase".into(),
                "--reviewed-by".into(),
                "human:local:test".into(),
                "--review-rationale".into(),
                "exact copy reviewed".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert_eq!(parsed.copy_fingerprint, Some("b".repeat(64)));
        assert_eq!(parsed.receipt_dir, Some(PathBuf::from("/receipts")));

        let adoption = parse_args(
            &[
                "--adopt-existing-fingerprint".into(),
                "e".repeat(64),
                "--receipt-dir".into(),
                "/receipts".into(),
                "--confirm-copy-phrase".into(),
                "exact adoption phrase".into(),
                "--reviewed-by".into(),
                "human:local:test".into(),
                "--review-rationale".into(),
                "exact adoption reviewed".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert_eq!(adoption.adopt_existing_fingerprint, Some("e".repeat(64)));
        assert!(validate_action_args(&adoption).is_ok());

        let mut conflicting_receipt_actions = adoption;
        conflicting_receipt_actions.copy_fingerprint = Some("f".repeat(64));
        assert!(validate_action_args(&conflicting_receipt_actions).is_err());

        let review = parse_args(
            &[
                "--review-candidate-fingerprint".into(),
                "c".repeat(64),
                "--review-fingerprint".into(),
                "d".repeat(64),
                "--review-disposition".into(),
                "approved".into(),
                "--reviewed-by".into(),
                "human:local:test".into(),
                "--review-rationale".into(),
                "metadata reviewed".into(),
                "--review-dir".into(),
                "/reviews".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert_eq!(review.review_candidate_fingerprint, Some("c".repeat(64)));
        assert_eq!(review.review_fingerprint, Some("d".repeat(64)));
        assert_eq!(
            review.review_disposition,
            Some(CloudReviewDisposition::Approved)
        );
        assert_eq!(review.review_dir, Some(PathBuf::from("/reviews")));
        assert_eq!(review.reviewed_by.as_deref(), Some("human:local:test"));
        assert_eq!(
            review.review_rationale.as_deref(),
            Some("metadata reviewed")
        );
        assert!(validate_action_args(&review).is_ok());

        let mut non_human_review = review.clone();
        non_human_review.reviewed_by = Some("agent:codex".into());
        assert_eq!(
            validate_action_args(&non_human_review).unwrap_err(),
            "cloud-review-decision-attribution-invalid"
        );

        let help = parse_args(&["--help".into()], Path::new("/h")).unwrap_err();
        assert!(help.contains("--reviewed-by human:ID"));
        assert!(help.contains("--confirm-copy-phrase EXACT"));
        assert!(help.contains("--provider-api-copy-fingerprint HEX64"));
        assert!(help.contains("--export-naruon-copy-readiness --verify-capacity"));
        assert!(help.contains("--naruon-copy-readiness-output ABSOLUTE_NEW_FILE.json"));
        assert!(help.contains("--private-candidate-inspection-output ABSOLUTE_NEW_FILE.json"));

        assert!(parse_args(
            &["--review-disposition".into(), "maybe".into(),],
            Path::new("/h"),
        )
        .is_err());
    }

    #[test]
    fn capacity_verification_allows_plan_and_copy_actions() {
        let mut plan = parse_args(&["--verify-capacity".into()], Path::new("/h")).unwrap();
        plan.oauth_connections = Some(PathBuf::from("/connections.json"));
        assert!(validate_action_args(&plan).is_ok());

        let mut copy = parse_args(&[], Path::new("/h")).unwrap();
        copy.copy_fingerprint = Some("a".repeat(64));
        copy.receipt_dir = Some(PathBuf::from("/receipts"));
        copy.confirm_copy_phrase = Some("exact copy phrase".into());
        copy.reviewed_by = Some("human:test".into());
        copy.review_rationale = Some("exact copy reviewed".into());
        copy.oauth_connections = Some(PathBuf::from("/connections.json"));
        assert!(validate_action_args(&copy).is_ok());

        let review = parse_args(
            &[
                "--verify-capacity".into(),
                "--review-candidate-fingerprint".into(),
                "c".repeat(64),
                "--review-fingerprint".into(),
                "d".repeat(64),
                "--review-disposition".into(),
                "approved".into(),
                "--reviewed-by".into(),
                "human:test".into(),
                "--review-rationale".into(),
                "provider scope and embedded metadata reviewed".into(),
                "--review-dir".into(),
                "/reviews".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&review).is_ok());

        let mut adoption = copy.clone();
        adoption.copy_fingerprint = None;
        adoption.adopt_existing_fingerprint = Some("b".repeat(64));
        assert!(validate_action_args(&adoption).is_err());

        plan.list_roots = true;
        assert!(validate_action_args(&plan).is_err());
    }

    #[test]
    fn capacity_verification_without_connection_is_a_redacted_blocked_assessment() {
        let root = CloudRoot {
            id: "onedrive:test".into(),
            provider: CloudProvider::Onedrive,
            account_scope: disksage_lib::cloud::CloudAccountScope::Personal,
            label: "OneDrive".into(),
            path: "/Cloud/OneDrive".into(),
            readable: true,
            access_issue: None,
        };

        let observed_at_ms = cloud::system_now_ms();
        let snapshot = match collect_root_capacity(&root, None, observed_at_ms) {
            Ok(snapshot) => snapshot,
            Err(error) => provider_capacity::unavailable_capacity_from_error(
                root.provider,
                observed_at_ms,
                &error,
            ),
        };
        let assessment = provider_capacity::assess_capacity(snapshot, 10, 10, 1024 * 1024);

        assert_eq!(assessment.can_fit, None);
        assert_eq!(
            assessment.snapshot.unavailable_reason.as_deref(),
            Some("provider-oauth-connection-missing")
        );
        assert_eq!(
            assessment.blockers,
            ["provider-oauth-connection-missing".to_string()]
        );
    }

    #[test]
    fn capacity_attachment_marks_each_non_oauth_destination_unavailable() {
        let root = CloudRoot {
            id: "onedrive:test".into(),
            provider: CloudProvider::Onedrive,
            account_scope: disksage_lib::cloud::CloudAccountScope::Personal,
            label: "OneDrive".into(),
            path: "/Cloud/OneDrive".into(),
            readable: true,
            access_issue: None,
        };
        let mut report = cloud::CloudPlanReport {
            cloud_root: root.clone(),
            generated_at_ms: 1,
            source_selection_policy: Some(cloud::CloudPlanOptions::default()),
            candidates: Vec::new(),
            candidate_bytes: 0,
            potentially_reclaimable_bytes: 0,
            exact_duplicates: cloud::ExactDuplicateSummary::default(),
            capacity: None,
            local_volume: None,
            pre_copy_evidence: None,
            notices: vec!["dry-run-only".into(), "cloud-quota-unverified".into()],
        };

        let snapshot = provider_capacity::unavailable_capacity_from_error(
            CloudProvider::Onedrive,
            1,
            "provider-capacity-oauth-connections-required",
        );
        attach_capacity_snapshot(&mut report, snapshot, 1024).unwrap();

        let assessment = report.capacity.unwrap();
        assert_eq!(assessment.can_fit, None);
        assert_eq!(
            assessment.blockers,
            ["provider-oauth-connection-missing".to_string()]
        );
        assert!(!report
            .notices
            .iter()
            .any(|notice| notice == "cloud-quota-unverified"));
        assert!(report
            .notices
            .iter()
            .any(|notice| notice == "cloud-quota-unavailable"));
    }

    #[test]
    fn action_validation_requires_complete_review_arguments() {
        let mut args = parse_args(&[], Path::new("/h")).unwrap();
        args.review_candidate_fingerprint = Some("c".repeat(64));
        assert!(validate_action_args(&args).is_err());
        args.review_fingerprint = Some("d".repeat(64));
        args.review_disposition = Some(CloudReviewDisposition::Held);
        assert!(validate_action_args(&args).is_err());
        args.reviewed_by = Some("human:local:test".into());
        args.review_rationale = Some("metadata reviewed".into());
        args.review_dir = Some(PathBuf::from("relative-reviews"));
        assert!(validate_action_args(&args).is_err());
        args.review_dir = Some(PathBuf::from("/reviews"));
        assert!(validate_action_args(&args).is_ok());

        args.copy_fingerprint = Some("a".repeat(64));
        args.receipt_dir = Some(PathBuf::from("/receipts"));
        assert!(validate_action_args(&args).is_err());

        args.review_candidate_fingerprint = None;
        args.review_fingerprint = None;
        args.review_disposition = None;
        args.confirm_copy_phrase = Some("exact copy phrase".into());
        args.reviewed_by = Some("human:local:test".into());
        args.review_rationale = Some("exact copy reviewed".into());
        assert!(validate_action_args(&args).is_ok());

        let mut reason_set = parse_args(
            &[
                "--review-reason-set".into(),
                "destination-account-scope-unknown".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&reason_set).is_err());
        reason_set.decision_summary = true;
        assert!(validate_action_args(&reason_set).is_ok());
    }

    #[test]
    fn naruon_export_requires_absolute_receipt_and_bound_optional_evidence() {
        let export = parse_args(
            &[
                "--export-naruon-lineage".into(),
                "/receipts/receipt.json".into(),
                "--naruon-sync-evidence".into(),
                "/evidence/evidence.json".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&export).is_ok());

        let relative = parse_args(
            &["--export-naruon-lineage".into(), "receipt.json".into()],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&relative).is_err());

        let evidence_only = parse_args(
            &[
                "--naruon-sync-evidence".into(),
                "/evidence/evidence.json".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&evidence_only).is_err());

        let mut conflicting = export;
        conflicting.list_roots = true;
        assert!(validate_action_args(&conflicting).is_err());
    }

    #[test]
    fn naruon_capacity_export_requires_fresh_single_destination_capacity() {
        let export = parse_args(
            &[
                "--verify-capacity".into(),
                "--export-naruon-capacity".into(),
                "--provider".into(),
                "icloud".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(export.export_naruon_capacity);
        assert!(validate_action_args(&export).is_ok());

        let missing_capacity =
            parse_args(&["--export-naruon-capacity".into()], Path::new("/h")).unwrap();
        assert!(validate_action_args(&missing_capacity).is_err());

        let multiple = parse_args(
            &[
                "--verify-capacity".into(),
                "--export-naruon-capacity".into(),
                "--all-readable-roots".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&multiple).is_err());
    }

    #[test]
    fn naruon_copy_readiness_export_is_fresh_single_destination_and_safe_output() {
        let export = parse_args(
            &[
                "--verify-capacity".into(),
                "--export-naruon-copy-readiness".into(),
                "--naruon-copy-readiness-output".into(),
                "/artifacts/readiness.json".into(),
                "--provider".into(),
                "onedrive".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(export.export_naruon_copy_readiness);
        assert_eq!(
            export.naruon_copy_readiness_output,
            Some(PathBuf::from("/artifacts/readiness.json"))
        );
        assert!(validate_action_args(&export).is_ok());

        let missing_capacity =
            parse_args(&["--export-naruon-copy-readiness".into()], Path::new("/h")).unwrap();
        assert!(validate_action_args(&missing_capacity).is_err());

        let output_only = parse_args(
            &[
                "--naruon-copy-readiness-output".into(),
                "/artifacts/readiness.json".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&output_only).is_err());

        let relative_output = parse_args(
            &[
                "--verify-capacity".into(),
                "--export-naruon-copy-readiness".into(),
                "--naruon-copy-readiness-output".into(),
                "readiness.json".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&relative_output).is_err());

        let mut conflicting = export;
        conflicting.export_naruon_capacity = true;
        assert!(validate_action_args(&conflicting).is_err());
    }

    #[test]
    fn semantic_catalog_export_is_single_destination_dry_run_only() {
        let export = parse_args(
            &[
                "--export-semantic-catalog".into(),
                "--provider".into(),
                "icloud".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(export.export_semantic_catalog);
        assert!(validate_action_args(&export).is_ok());

        let mut conflicting = export.clone();
        conflicting.copy_fingerprint = Some("a".repeat(64));
        conflicting.receipt_dir = Some(PathBuf::from("/receipts"));
        assert!(validate_action_args(&conflicting).is_err());

        let multiple = parse_args(
            &[
                "--export-semantic-catalog".into(),
                "--all-readable-roots".into(),
                "--decision-summary".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&multiple).is_err());

        let summary = parse_args(
            &[
                "--export-semantic-catalog".into(),
                "--decision-summary".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&summary).is_err());
    }

    #[test]
    fn action_validation_requires_explicit_complete_eviction_arguments() {
        let mut args = parse_args(&[], Path::new("/h")).unwrap();
        args.evict_receipt = Some(PathBuf::from("/receipts/a.json"));
        assert!(validate_action_args(&args).is_err());
        args.confirm_receipt_id = Some("a".repeat(64));
        args.eviction_dir = Some(PathBuf::from("/evictions"));
        args.eviction_approval_dir = Some(PathBuf::from("/approvals"));
        args.journal_path = Some(PathBuf::from("relative-journal"));
        assert!(validate_action_args(&args).is_err());
        args.journal_path = Some(PathBuf::from("/journal/operations.jsonl"));
        args.evidence_dir = Some(PathBuf::from("relative-evidence"));
        assert!(validate_action_args(&args).is_err());
        args.evidence_dir = Some(PathBuf::from("/evidence"));
        args.reviewed_by = Some("human:local:test".into());
        args.review_rationale = Some("verified exact receipt source".into());
        assert!(validate_action_args(&args).is_ok());

        args.attest_receipt = Some(PathBuf::from("/receipt.json"));
        assert!(validate_action_args(&args).is_err());

        let parsed = parse_args(
            &[
                "--evict-receipt".into(),
                "/receipts/a.json".into(),
                "--confirm-receipt-id".into(),
                "b".repeat(64),
                "--eviction-dir".into(),
                "/evictions".into(),
                "--eviction-approval-dir".into(),
                "/approvals".into(),
                "--journal-path".into(),
                "/journal/operations.jsonl".into(),
                "--evidence-dir".into(),
                "/evidence".into(),
                "--reviewed-by".into(),
                "human:local:test".into(),
                "--review-rationale".into(),
                "verified exact receipt source".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert_eq!(parsed.confirm_receipt_id, Some("b".repeat(64)));
        assert!(validate_action_args(&parsed).is_ok());
    }

    #[test]
    fn provider_api_fallback_requires_complete_scoped_arguments() {
        let parsed = parse_args(
            &[
                "--attest-receipt".into(),
                "/receipts/a.json".into(),
                "--provider-object-id".into(),
                "remote-item-id".into(),
                "--oauth-connections".into(),
                "/app-data/cloud-oauth-connections.json".into(),
                "--evidence-dir".into(),
                "/evidence".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert_eq!(parsed.provider_object_id.as_deref(), Some("remote-item-id"));
        assert_eq!(
            parsed.oauth_connections,
            Some(PathBuf::from("/app-data/cloud-oauth-connections.json"))
        );
        assert!(validate_action_args(&parsed).is_ok());

        let onedrive_path_fallback = parse_args(
            &[
                "--attest-receipt".into(),
                "/receipts/a.json".into(),
                "--oauth-connections".into(),
                "/app-data/cloud-oauth-connections.json".into(),
                "--evidence-dir".into(),
                "/evidence".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&onedrive_path_fallback).is_ok());

        let mut provider_api_copy = parse_args(
            &[
                "--provider-api-copy-fingerprint".into(),
                "f".repeat(64),
                "--receipt-dir".into(),
                "/receipts".into(),
                "--confirm-copy-phrase".into(),
                "exact provider api copy phrase".into(),
                "--reviewed-by".into(),
                "human:local:test".into(),
                "--review-rationale".into(),
                "provider API path reviewed".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&provider_api_copy).is_err());
        provider_api_copy.oauth_connections = Some(PathBuf::from("/connections.json"));
        assert!(validate_action_args(&provider_api_copy).is_ok());
        provider_api_copy.provider_object_id = Some("unexpected-id".into());
        assert!(validate_action_args(&provider_api_copy).is_err());

        let mut incomplete = parse_args(
            &[
                "--attest-receipt".into(),
                "/receipts/a.json".into(),
                "--provider-object-id".into(),
                "remote-item-id".into(),
                "--evidence-dir".into(),
                "/evidence".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&incomplete).is_err());
        incomplete.oauth_connections = Some(PathBuf::from("relative-connections.json"));
        assert!(validate_action_args(&incomplete).is_err());

        let mut unscoped = parse_args(&[], Path::new("/h")).unwrap();
        unscoped.provider_object_id = Some("remote-item-id".into());
        unscoped.oauth_connections = Some(PathBuf::from("/connections.json"));
        assert!(validate_action_args(&unscoped).is_err());
    }

    #[cfg(not(coverage))]
    #[test]
    fn icloud_reconciliation_uses_native_probe_with_shared_oauth_descriptor() {
        let receipt = CloudCopyReceipt {
            version: cloud_transfer::RECEIPT_VERSION,
            receipt_id: "0".repeat(64),
            candidate_fingerprint: "1".repeat(64),
            provider: CloudProvider::Icloud,
            source: "/source/file.bin".into(),
            destination: "/missing/icloud/file.bin".into(),
            bytes: 1,
            blake3: "2".repeat(64),
            sha256: "3".repeat(64),
            quick_xor_base64: String::new(),
            source_modified_ms: 1,
            copied_at_ms: 2,
            copy_verified: true,
            provider_sync_confirmed: false,
            lineage_fingerprint: None,
            lineage: None,
        };
        let error = collect_receipt_sync_evidence(
            &receipt,
            None,
            Some(Path::new("/connections.json")),
            Path::new("/home/test"),
            3,
            false,
        )
        .unwrap_err();
        assert_ne!(error, "icloud-provider-api-fallback-not-supported");
    }

    #[test]
    fn attestation_rejects_forged_receipt_before_destination_probe() {
        let temp = tempfile::tempdir().unwrap();
        let receipt = CloudCopyReceipt {
            version: cloud_transfer::RECEIPT_VERSION,
            receipt_id: "0".repeat(64),
            candidate_fingerprint: "1".repeat(64),
            provider: CloudProvider::Icloud,
            source: temp
                .path()
                .join("source.pdf")
                .to_string_lossy()
                .into_owned(),
            destination: temp
                .path()
                .join("destination-does-not-exist.pdf")
                .to_string_lossy()
                .into_owned(),
            bytes: 1,
            blake3: "2".repeat(64),
            sha256: "3".repeat(64),
            quick_xor_base64: "AAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
            source_modified_ms: 1,
            copied_at_ms: 2,
            copy_verified: true,
            provider_sync_confirmed: false,
            lineage_fingerprint: None,
            lineage: None,
        };
        let path = temp.path().join(format!("{}.json", receipt.receipt_id));
        std::fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();

        let error =
            attest_receipt(&path, temp.path(), None, None, Path::new("/home/test")).unwrap_err();
        assert!(error.contains("receipt-integrity-mismatch"));
        assert!(!error.contains("No such file"));
    }
}
