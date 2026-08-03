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
use disksage_lib::cloud::{
    self, ArchiveKind, CloudAccountScope, CloudPlanOptions, CloudProvider, CloudRoot,
};
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
#[cfg(not(coverage))]
use disksage_lib::naruon_duplicate_audit_lineage::NaruonDuplicateAuditLineageEnvelope;
#[cfg(not(coverage))]
use disksage_lib::naruon_duplicate_canonical_review;
#[cfg(not(coverage))]
use disksage_lib::naruon_lineage;
#[cfg(not(coverage))]
use disksage_lib::provider_api_client::{self, FixedHostProviderMetadataClient};
#[cfg(not(coverage))]
use disksage_lib::provider_capacity;
#[cfg(not(coverage))]
use disksage_lib::provider_client_runtime;
#[cfg(not(coverage))]
use disksage_lib::provider_evidence::{self, ProviderSyncEvidenceRecord};
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
    capacity_readiness_summary: bool,
    review_reason_set: Option<Vec<String>>,
    private_review_output: Option<PathBuf>,
    private_full_review_output: Option<PathBuf>,
    exact_duplicate_review_prefix: Option<String>,
    exact_duplicate_kind: Option<ArchiveKind>,
    capacity_reserve_mib: u64,
    copy_fingerprint: Option<String>,
    adopt_existing_fingerprint: Option<String>,
    receipt_dir: Option<PathBuf>,
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
    export_naruon_duplicate_canonical_review: Option<PathBuf>,
    duplicate_canonical_review_output: Option<PathBuf>,
    duplicate_canonical_review_dossier_output: Option<PathBuf>,
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
        capacity_readiness_summary: false,
        review_reason_set: None,
        private_review_output: None,
        private_full_review_output: None,
        exact_duplicate_review_prefix: None,
        exact_duplicate_kind: None,
        capacity_reserve_mib: 1024,
        copy_fingerprint: None,
        adopt_existing_fingerprint: None,
        receipt_dir: None,
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
        export_naruon_duplicate_canonical_review: None,
        duplicate_canonical_review_output: None,
        duplicate_canonical_review_dossier_output: None,
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
            "--capacity-readiness-summary" => parsed.capacity_readiness_summary = true,
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
            "--private-full-review-output" => {
                if parsed.private_full_review_output.is_some() {
                    return Err(
                        "--private-full-review-output은 한 번만 지정할 수 있음"
                            .into(),
                    );
                }
                parsed.private_full_review_output =
                    Some(PathBuf::from(value(
                        args,
                        &mut index,
                        "--private-full-review-output",
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
            "--export-naruon-duplicate-canonical-review" => {
                if parsed.export_naruon_duplicate_canonical_review.is_some() {
                    return Err(
                        "--export-naruon-duplicate-canonical-review는 한 번만 지정할 수 있음"
                            .into(),
                    );
                }
                parsed.export_naruon_duplicate_canonical_review = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--export-naruon-duplicate-canonical-review",
                )?));
            }
            "--duplicate-canonical-review-output" => {
                if parsed.duplicate_canonical_review_output.is_some() {
                    return Err(
                        "--duplicate-canonical-review-output은 한 번만 지정할 수 있음".into(),
                    );
                }
                parsed.duplicate_canonical_review_output = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--duplicate-canonical-review-output",
                )?));
            }
            "--duplicate-canonical-review-dossier-output" => {
                if parsed.duplicate_canonical_review_dossier_output.is_some() {
                    return Err(
                        "--duplicate-canonical-review-dossier-output은 한 번만 지정할 수 있음"
                            .into(),
                    );
                }
                parsed.duplicate_canonical_review_dossier_output =
                    Some(PathBuf::from(value(
                        args,
                        &mut index,
                        "--duplicate-canonical-review-dossier-output",
                    )?));
            }
            "--export-semantic-catalog" => parsed.export_semantic_catalog = true,
            "--help" | "-h" => {
                return Err(
                    "usage: disksage-cloud-plan [--list-roots | --inspect-roots] [--root PATH] [--cloud-root PATH | --provider icloud|onedrive|google-drive | --all-readable-roots --decision-summary [--capacity-readiness-summary --verify-capacity]] [--min-size-mib N] [--min-age-days N] [--limit N] [--decision-summary [--private-full-review-output ABSOLUTE_NEW_FILE.json | --review-reason-set REASON|REASON [--private-review-output ABSOLUTE_NEW_FILE.json]] | --exact-duplicate-review-prefix DIR_PREFIX --exact-duplicate-kind document|media|archive|dataset|backup|creative|incomplete-download | --export-naruon-copy-readiness --verify-capacity [--naruon-copy-readiness-output ABSOLUTE_NEW_FILE.json] | --export-semantic-catalog | --export-naruon-duplicate-canonical-review AUDIT_LINEAGE.json [--duplicate-canonical-review-output ABSOLUTE_NEW_FILE.json] [--duplicate-canonical-review-dossier-output ABSOLUTE_NEW_FILE.json]] [--verify-capacity [--oauth-connections ABSOLUTE_PATH] [--export-naruon-capacity]] [--capacity-reserve-mib N] [--copy-fingerprint HEX64 --receipt-dir PATH [--review-dir PATH] [--oauth-connections ABSOLUTE_PATH] | --adopt-existing-fingerprint HEX64 --receipt-dir PATH [--review-dir PATH] | --attest-receipt RECEIPT.json --evidence-dir ABSOLUTE_PATH [--oauth-connections ABSOLUTE_PATH [--provider-object-id GOOGLE_FILE_ID]] | --evict-receipt RECEIPT.json --confirm-receipt-id HEX64 --eviction-dir ABSOLUTE_PATH --eviction-approval-dir ABSOLUTE_PATH --journal-path ABSOLUTE_PATH --evidence-dir ABSOLUTE_PATH --reviewed-by human:ID --review-rationale TEXT [--oauth-connections ABSOLUTE_PATH [--provider-object-id GOOGLE_FILE_ID]] | --review-candidate-fingerprint HEX64 --review-fingerprint HEX64 --review-disposition approved|held --reviewed-by human:ID --review-rationale TEXT --review-dir PATH | --export-naruon-lineage RECEIPT.json [--naruon-sync-evidence EVIDENCE.json]]".into(),
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
    receipt: CloudCopyReceipt,
    receipt_path: String,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct AttestationOutput {
    action: &'static str,
    receipt_id: String,
    evidence: disksage_lib::cloud_transfer::ProviderSyncEvidence,
    assessment: provider_sync::ProviderSyncTimelinessAssessment,
    evidence_record: ProviderSyncEvidenceRecord,
    evidence_path: String,
    permit: Option<LocalEvictionPermit>,
    blockers: Vec<String>,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct EvictionOutput {
    action: &'static str,
    receipt_id: String,
    evidence: disksage_lib::cloud_transfer::ProviderSyncEvidence,
    evidence_record: ProviderSyncEvidenceRecord,
    evidence_path: String,
    permit: LocalEvictionPermit,
    approval: CloudSourceEvictionApproval,
    approval_path: String,
    eviction: CloudEvictionResult,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct ReviewOutput {
    action: &'static str,
    decision: CloudReviewDecision,
    decision_path: String,
}

#[cfg(not(coverage))]
fn validate_action_args(args: &Args) -> Result<(), String> {
    let copy_action = args.copy_fingerprint.is_some();
    let adoption_action = args.adopt_existing_fingerprint.is_some();
    let exact_duplicate_review =
        args.exact_duplicate_review_prefix.is_some() || args.exact_duplicate_kind.is_some();
    let duplicate_canonical_review = args
        .export_naruon_duplicate_canonical_review
        .is_some();
    if args.exact_duplicate_review_prefix.is_some() != args.exact_duplicate_kind.is_some() {
        return Err(
            "--exact-duplicate-review-prefix와 --exact-duplicate-kind는 함께 지정해야 함".into(),
        );
    }
    if args.all_readable_roots && !args.decision_summary {
        return Err("--all-readable-roots에는 --decision-summary가 필요함".into());
    }
    if args.capacity_readiness_summary
        && (!args.all_readable_roots || !args.decision_summary || !args.verify_capacity)
    {
        return Err(
            "--capacity-readiness-summary에는 --all-readable-roots, --decision-summary, --verify-capacity가 필요함"
                .into(),
        );
    }
    if args.capacity_readiness_summary && args.review_reason_set.is_some() {
        return Err(
            "--capacity-readiness-summary는 파일별 review batch와 함께 사용할 수 없음".into(),
        );
    }
    if args.all_readable_roots && (args.cloud_root.is_some() || args.provider.is_some()) {
        return Err(
            "--all-readable-roots는 --cloud-root 또는 --provider와 함께 사용할 수 없음".into(),
        );
    }
    if copy_action && adoption_action {
        return Err("copy action과 existing-copy adoption action은 동시에 사용할 수 없음".into());
    }
    if (copy_action || adoption_action) != args.receipt_dir.is_some() {
        return Err("copy/adoption fingerprint와 --receipt-dir은 함께 지정해야 함".into());
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
    if attributed && !review_action && !eviction_action {
        return Err("reviewer와 rationale는 review 또는 eviction action에만 지정할 수 있음".into());
    }
    let attestation_action = args.attest_receipt.is_some();
    if (attestation_action || eviction_action) != args.evidence_dir.is_some() {
        return Err("attestation/eviction action에는 --evidence-dir이 반드시 필요함".into());
    }
    if args.provider_object_id.is_some() && args.oauth_connections.is_none() {
        return Err("--provider-object-id에는 --oauth-connections가 필요함".into());
    }
    let remote_provider_api = args.oauth_connections.is_some();
    if remote_provider_api
        && args.attest_receipt.is_none()
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
            || exact_duplicate_review
            || duplicate_canonical_review
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
        return Err(
            "--export-naruon-copy-readiness에는 --verify-capacity가 필요함"
                .into(),
        );
    }
    if args.naruon_copy_readiness_output.is_some()
        && !args.export_naruon_copy_readiness
    {
        return Err(
            "--naruon-copy-readiness-output에는 --export-naruon-copy-readiness가 필요함"
                .into(),
        );
    }
    if args
        .naruon_copy_readiness_output
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err(
            "--naruon-copy-readiness-output은 절대 경로여야 함".into(),
        );
    }
    if args.duplicate_canonical_review_output.is_some() && !duplicate_canonical_review {
        return Err(
            "--duplicate-canonical-review-output에는 --export-naruon-duplicate-canonical-review가 필요함"
                .into(),
        );
    }
    if args.duplicate_canonical_review_dossier_output.is_some()
        && !duplicate_canonical_review
    {
        return Err(
            "--duplicate-canonical-review-dossier-output에는 --export-naruon-duplicate-canonical-review가 필요함"
                .into(),
        );
    }
    let actions = usize::from(args.list_roots)
        + usize::from(args.inspect_roots)
        + usize::from(copy_action)
        + usize::from(adoption_action)
        + usize::from(args.attest_receipt.is_some())
        + usize::from(eviction_action)
        + usize::from(review_action)
        + usize::from(args.export_naruon_lineage.is_some())
        + usize::from(args.export_naruon_capacity)
        + usize::from(args.export_naruon_copy_readiness)
        + usize::from(duplicate_canonical_review)
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
    if args.private_full_review_output.is_some()
        && (!args.decision_summary
            || args.review_reason_set.is_some()
            || args.private_review_output.is_some())
    {
        return Err(
            "--private-full-review-output은 전체 --decision-summary에서만 단독으로 사용할 수 있음"
                .into(),
        );
    }
    if args.private_full_review_output.is_some() && args.all_readable_roots {
        return Err(
            "--private-full-review-output은 단일 cloud destination에서만 사용할 수 있음"
                .into(),
        );
    }
    if actions > 1 {
        return Err(
            "root inspection, copy, adoption, attestation, eviction, review action은 동시에 사용할 수 없음".into(),
        );
    }
    for (flag, fingerprint) in [
        ("--copy-fingerprint", args.copy_fingerprint.as_ref()),
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
    if let Some(output_path) = &args.private_full_review_output {
        if !output_path.is_absolute() {
            return Err("--private-full-review-output은 절대 경로여야 함".into());
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
        (
            "--export-naruon-duplicate-canonical-review",
            &args.export_naruon_duplicate_canonical_review,
        ),
        (
            "--duplicate-canonical-review-output",
            &args.duplicate_canonical_review_output,
        ),
        (
            "--duplicate-canonical-review-dossier-output",
            &args.duplicate_canonical_review_dossier_output,
        ),
    ] {
        if path.as_ref().is_some_and(|path| !path.is_absolute()) {
            return Err(format!("{flag}는 절대 경로여야 함"));
        }
    }
    Ok(())
}

#[cfg(not(coverage))]
fn read_duplicate_audit_lineage(
    path: &Path,
) -> Result<NaruonDuplicateAuditLineageEnvelope, String> {
    const MAX_BYTES: u64 = 2 * 1024 * 1024;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "duplicate-canonical-audit-lineage-unavailable".to_string())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > MAX_BYTES
    {
        return Err("duplicate-canonical-audit-lineage-unsafe-or-unbounded".into());
    }
    let bytes = std::fs::read(path)
        .map_err(|_| "duplicate-canonical-audit-lineage-read-failed".to_string())?;
    serde_json::from_slice(&bytes)
        .map_err(|_| "duplicate-canonical-audit-lineage-json-invalid".to_string())
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
    let mut archive_kind_counts = BTreeMap::new();
    let mut archive_kind_candidate_bytes = BTreeMap::new();
    let mut production_month_counts = BTreeMap::new();
    let mut production_month_candidate_bytes = BTreeMap::new();
    let mut destination_bucket_counts = BTreeMap::new();
    let mut destination_bucket_candidate_bytes = BTreeMap::new();
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
        let kind = archive_kind_label(candidate.kind);
        let (year, month, _) = cloud::production_date_parts(candidate.production_time_ms);
        let production_month = format!("{year:04}-{month:02}");
        let destination_bucket = format!("{year:04}/{month:02}/{kind}");
        increment(&mut decision_state_counts, state, 1);
        increment(&mut decision_state_candidate_bytes, state, candidate.bytes);
        increment(&mut archive_kind_counts, kind, 1);
        increment(
            &mut archive_kind_candidate_bytes,
            kind,
            candidate.bytes,
        );
        increment(&mut production_month_counts, &production_month, 1);
        increment(
            &mut production_month_candidate_bytes,
            &production_month,
            candidate.bytes,
        );
        increment(&mut destination_bucket_counts, &destination_bucket, 1);
        increment(
            &mut destination_bucket_candidate_bytes,
            &destination_bucket,
            candidate.bytes,
        );
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
        "archive_kind": {
            "counts": archive_kind_counts,
            "candidate_bytes": archive_kind_candidate_bytes,
        },
        "production_month": {
            "counts": production_month_counts,
            "candidate_bytes": production_month_candidate_bytes,
            "timezone": "UTC",
        },
        "destination_bucket": {
            "counts": destination_bucket_counts,
            "candidate_bytes": destination_bucket_candidate_bytes,
            "relative_layout": "{YYYY}/{MM}/{kind}",
            "source_relative_path_redacted": true,
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
    serde_json::json!({
        "metadata_fingerprint": &candidate.metadata_fingerprint,
        "review_fingerprint": &candidate.review_fingerprint,
        "relative_path": &candidate.relative_path,
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
        "schema_version": 1,
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
        },
        "redacted_from_summary": [
            "absolute-source-path",
            "absolute-destination-path",
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

/// Build one bounded local-only dossier for every candidate in the fresh single-destination plan.
///
/// Blocked candidates remain present so an operator can understand incomplete downloads,
/// multipart sets, unreadable archives, and destination conflicts without confusing those states
/// with review approval. The dossier is sensitive and must use the mode-0600 create-new writer.
#[cfg(not(coverage))]
fn private_full_review_dossier(report: &cloud::CloudPlanReport) -> serde_json::Value {
    let mut candidates = report.candidates.iter().collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.metadata_fingerprint
            .cmp(&right.metadata_fingerprint)
            .then_with(|| left.review_fingerprint.cmp(&right.review_fingerprint))
    });
    let review_required_count = candidates
        .iter()
        .filter(|candidate| candidate_decision_state(candidate) == "review-required")
        .count();
    let blocked_count = candidates
        .iter()
        .filter(|candidate| candidate_decision_state(candidate) == "blocked")
        .count();

    serde_json::json!({
        "schema_version": 1,
        "output_mode": "private-full-review-dossier",
        "generated_at_ms": report.generated_at_ms,
        "contains_sensitive_local_metadata": true,
        "source_selection_policy": report.source_selection_policy,
        "decision_batch_fingerprint_version": cloud::CLOUD_DECISION_BATCH_FINGERPRINT_VERSION,
        "decision_batch_fingerprint": cloud::cloud_decision_batch_fingerprint(report),
        "dossier_scope": "all-fresh-plan-candidates-including-blocked",
        "cloud_root": &report.cloud_root,
        "candidate_count": candidates.len(),
        "candidate_bytes": report.candidate_bytes,
        "potentially_reclaimable_bytes": report.potentially_reclaimable_bytes,
        "review_required_count": review_required_count,
        "blocked_count": blocked_count,
        "aggregates": decision_aggregates(report),
        "exact_duplicates": &report.exact_duplicates,
        "capacity": &report.capacity,
        "notices": &report.notices,
        "metadata_policy": {
            "production_time_precedence": [
                "embedded-metadata",
                "explicit-filename-date",
                "filesystem-created",
                "filesystem-modified",
            ],
            "filename_dates_are_auxiliary": true,
            "raw_metadata_values_are_review_evidence_only": true,
            "blocked_candidates_are_not_approval_eligible": true,
            "dossier_is_not_approval": true,
            "candidate_review_decisions_remain_individual": true,
        },
        "provider_sync_attested": false,
        "cloud_write_performed": false,
        "source_eviction_authorized": false,
        "candidates": candidates,
    })
}

/// Return only path-free aggregates and the integrity metadata of the separately written dossier.
#[cfg(not(coverage))]
fn private_full_review_summary(
    report: &cloud::CloudPlanReport,
    dossier_sha256: &str,
    dossier_bytes: usize,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "output_mode": "private-full-review-summary",
        "generated_at_ms": report.generated_at_ms,
        "source_selection_policy": report.source_selection_policy,
        "decision_batch_fingerprint_version": cloud::CLOUD_DECISION_BATCH_FINGERPRINT_VERSION,
        "decision_batch_fingerprint": cloud::cloud_decision_batch_fingerprint(report),
        "cloud": {
            "provider": report.cloud_root.provider,
            "account_scope": report.cloud_root.account_scope,
        },
        "candidate_count": report.candidates.len(),
        "candidate_bytes": report.candidate_bytes,
        "potentially_reclaimable_bytes": report.potentially_reclaimable_bytes,
        "aggregates": decision_aggregates(report),
        "capacity": &report.capacity,
        "notices": &report.notices,
        "metadata_policy": {
            "production_time_precedence": [
                "embedded-metadata",
                "explicit-filename-date",
                "filesystem-created",
                "filesystem-modified",
            ],
            "filename_dates_are_auxiliary": true,
            "summary_is_dry_run_only": true,
            "blocked_candidates_are_not_approval_eligible": true,
            "candidate_review_decisions_remain_individual": true,
        },
        "private_full_review_dossier": {
            "written": true,
            "bytes": dossier_bytes,
            "sha256": dossier_sha256,
            "unix_mode": "0600",
            "create_new": true,
            "contains_sensitive_local_metadata": true,
            "checksum_is_not_signature": true,
            "is_approval": false,
        },
        "redacted_from_summary": [
            "absolute-source-path",
            "absolute-destination-path",
            "relative-source-name",
            "cloud-root-path-and-label",
            "content-title-and-authors",
            "raw-metadata-evidence-values",
            "dataset-profile",
            "exact-duplicate-members",
        ],
        "paths_redacted": true,
        "relative_names_redacted": true,
        "provider_sync_attested": false,
        "cloud_write_performed": false,
        "source_eviction_authorized": false,
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

/// Produce a bounded operator view without absolute paths or raw embedded metadata values.
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
        "schema_version": 1,
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
            "verified_provider_sync_required_before_local_eviction": true,
            "destination_layout":
                "DiskSage Archive/{YYYY}/{MM}/{kind}/{source-relative-path}",
            "placement_aggregates_are_path_free": true,
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
            "cloud-root-path-and-label",
            "content-title-and-authors",
            "raw-metadata-evidence-values",
            "dataset-profile",
        ],
        "decisions": decisions,
    })
}

/// Produce a public, path-free capacity view.
///
/// A decision summary deliberately keeps relative names so a human can review individual files.
/// Capacity readiness only needs aggregates, so it drops those decisions and their approval
/// fingerprints as well.
#[cfg(not(coverage))]
fn capacity_readiness_destination_summary(report: &cloud::CloudPlanReport) -> serde_json::Value {
    let mut summary = decision_summary(report);
    let object = summary
        .as_object_mut()
        .expect("decision summary is always a JSON object");
    object.insert(
        "output_mode".into(),
        serde_json::Value::String("capacity-readiness-destination-summary".into()),
    );
    object.remove("decisions");
    object.remove("decision_batch_fingerprint");
    object.remove("decision_batch_fingerprint_version");
    object.insert("paths_redacted".into(), serde_json::Value::Bool(true));
    object.insert(
        "relative_names_redacted".into(),
        serde_json::Value::Bool(true),
    );
    summary
}

#[cfg(not(coverage))]
fn has_verified_capacity(report: &cloud::CloudPlanReport) -> bool {
    report.capacity.as_ref().is_some_and(|assessment| {
        assessment.snapshot.evidence_kind != provider_capacity::CapacityEvidenceKind::Unavailable
            && assessment.can_fit.is_some()
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
fn collect_root_capacity(
    root: &CloudRoot,
    oauth_connections: Option<&Path>,
    observed_at_ms: u64,
) -> Result<provider_capacity::CloudCapacitySnapshot, String> {
    provider_capacity::collect_live_root_capacity(root, oauth_connections, observed_at_ms)
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
    if report.cloud_root.provider == CloudProvider::Icloud {
        let health =
            icloud_sync_health::inspect_new_copy_admission(home, cloud::system_now_ms()).ok();
        icloud_sync_health::attach_new_copy_admission_notice(
            &mut report.notices,
            health.as_ref(),
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
) -> Result<disksage_lib::cloud_transfer::ProviderSyncEvidence, String> {
    let provider_object_id = provider_object_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match receipt.provider {
        CloudProvider::Icloud => {
            if provider_object_id.is_some() || oauth_connections.is_some() {
                return Err("icloud-provider-api-fallback-not-supported".into());
            }
            provider_sync::collect_icloud_sync_evidence(receipt, confirmed_at_ms)
        }
        CloudProvider::Onedrive | CloudProvider::GoogleDrive => {
            let fallback_requested = oauth_connections.is_some();
            match provider_sync::collect_file_provider_sync_evidence(receipt, confirmed_at_ms) {
                Ok(evidence) if evidence.sync_complete || !fallback_requested => Ok(evidence),
                Err(error) if !fallback_requested => Err(error),
                Ok(_) | Err(_) => {
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
}

#[cfg(not(coverage))]
fn attest_receipt(
    path: &Path,
    evidence_dir: &Path,
    provider_object_id: Option<&str>,
    oauth_connections: Option<&Path>,
    home: &Path,
) -> Result<AttestationOutput, String> {
    let receipt = cloud_transfer::read_immutable_receipt(path)?;
    let confirmed_at_ms = cloud::system_now_ms();
    let evidence = collect_receipt_sync_evidence(
        &receipt,
        provider_object_id,
        oauth_connections,
        home,
        confirmed_at_ms,
    )?;
    let assessment = provider_sync::assess_provider_sync_timeliness(&receipt, &evidence)?;
    let (evidence_record, evidence_path) =
        provider_evidence::write_immutable_sync_evidence(evidence_dir, &evidence)?;
    let (permit, blockers) =
        match cloud_transfer::approve_local_eviction(&receipt, &evidence_record) {
            Ok(permit) => (Some(permit), Vec::new()),
            Err(blockers) => (None, blockers),
        };
    Ok(AttestationOutput {
        action: "attest-provider-native",
        receipt_id: receipt.receipt_id,
        evidence,
        assessment,
        evidence_record,
        evidence_path: evidence_path.to_string_lossy().into_owned(),
        permit,
        blockers,
    })
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
    )?;
    let (evidence_record, evidence_path) =
        provider_evidence::write_immutable_sync_evidence(evidence_dir, &evidence)?;
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
    Ok(EvictionOutput {
        action: "attest-and-trash-verified-cloud-source",
        receipt_id: receipt.receipt_id,
        evidence,
        evidence_record,
        evidence_path: evidence_path.to_string_lossy().into_owned(),
        permit,
        approval,
        approval_path: approval_path.to_string_lossy().into_owned(),
        eviction,
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
    let files = cloud::collect_archive_files(&args.root, &excluded);
    let snapshot = cloud::prepare_cloud_archive_source(
        &files,
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
        let mut capacity_verified_destination_count = 0usize;
        for selected in &selected_roots {
            let (_, report) = plan_with_optional_capacity(
                &snapshot,
                selected,
                args.verify_capacity,
                args.oauth_connections.as_deref(),
                args.capacity_reserve_mib,
                &home,
            )?;
            capacity_verified_destination_count += usize::from(has_verified_capacity(&report));
            summaries.push(if args.capacity_readiness_summary {
                capacity_readiness_destination_summary(&report)
            } else {
                match args.review_reason_set.as_deref() {
                    Some(reasons) => review_batch_summary(&report, reasons)?,
                    None => decision_summary(&report),
                }
            });
        }
        let capacity_notice = if args.verify_capacity {
            "cloud-capacity-assessed-per-destination"
        } else {
            "cloud-capacity-unverified"
        };
        let output = if args.capacity_readiness_summary {
            serde_json::json!({
                "schema_version": 2,
                "output_mode": "multicloud-capacity-readiness-summary",
                "source_snapshot": {
                    "candidate_count": snapshot.candidate_count(),
                    "candidate_bytes": snapshot.candidate_bytes(),
                    "reused_for_destination_count": selected_roots.len(),
                    "content_metadata_probed_once": true,
                    "duplicate_content_hashed_once": true,
                },
                "metadata_policy": {
                    "production_time_precedence": [
                        "embedded-metadata",
                        "explicit-filename-date",
                        "filesystem-created",
                        "filesystem-modified",
                    ],
                    "filename_dates_are_auxiliary": true,
                },
                "destinations": summaries,
                "capacity_verified_destination_count": capacity_verified_destination_count,
                "remote_capacity_verified_for_all_destinations":
                    capacity_verified_destination_count == selected_roots.len(),
                "paths_redacted": true,
                "relative_names_redacted": true,
                "provider_sync_attested": false,
                "mutation_performed": false,
                "cloud_write_performed": false,
                "source_eviction_authorized": false,
                "notices": [
                    "dry-run-only",
                    "destination-state-revalidated-per-plan",
                    "source-stat-revalidated-per-plan",
                    capacity_notice,
                    "cloud-sync-unverified",
                ],
            })
        } else {
            serde_json::json!({
                "schema_version": 1,
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
            })
        };
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
    let capacity_required_for_plan = args.verify_capacity || args.copy_fingerprint.is_some();
    let (selected, report) = plan_with_optional_capacity(
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
            icloud_sync_health::inspect_new_copy_admission(
                &home,
                observed_at_ms,
            )
            .ok()
        } else {
            None
        };
        let envelope =
            naruon_cloud_copy_readiness::export_naruon_cloud_copy_readiness(
                &report,
                &runtime,
                icloud_health.as_ref(),
            )?;
        if let Some(output_path) = &args.naruon_copy_readiness_output {
            let value = serde_json::to_value(&envelope).map_err(|_| {
                "naruon-copy-readiness-output-json-invalid".to_string()
            })?;
            write_private_review_dossier(output_path, &value)?;
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope)
                .map_err(|error| error.to_string())?
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
    if let Some(audit_path) = &args.export_naruon_duplicate_canonical_review {
        let audit = read_duplicate_audit_lineage(audit_path)?;
        let envelope =
            naruon_duplicate_canonical_review::export_naruon_duplicate_canonical_review(
                &report,
                &audit,
                cloud::system_now_ms(),
            )?;
        if let Some(output_path) = &args.duplicate_canonical_review_output {
            let value = serde_json::to_value(&envelope)
                .map_err(|_| "duplicate-canonical-review-json-invalid".to_string())?;
            write_private_review_dossier(output_path, &value)?;
        }
        if let Some(output_path) = &args.duplicate_canonical_review_dossier_output {
            let dossier =
                naruon_duplicate_canonical_review::export_local_duplicate_canonical_review_dossier(
                    &report, &envelope,
                )?;
            let value = serde_json::to_value(&dossier)
                .map_err(|_| "duplicate-canonical-review-dossier-json-invalid".to_string())?;
            write_private_review_dossier(output_path, &value)?;
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&envelope).map_err(|error| error.to_string())?
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
        if !adopt_existing {
            provider_client_runtime::require_provider_client_runtime(
                selected.provider,
                cloud::system_now_ms(),
            )?;
            if selected.provider == CloudProvider::Icloud {
                let health = icloud_sync_health::inspect_new_copy_admission(
                    &home,
                    cloud::system_now_ms(),
                )
                .map_err(|_| "icloud-new-copy-admission-evidence-unavailable".to_string())?;
                icloud_sync_health::require_new_copy_admission(&health)?;
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
            if assessment.can_fit != Some(true) {
                return Err(if assessment.blockers.is_empty() {
                    "cloud-capacity-verification-required".into()
                } else {
                    assessment.blockers.join(",")
                });
            }
        }
        let (receipt, receipt_path) = if adopt_existing {
            cloud_transfer::adopt_existing_cloud_copy_with_review(
                candidate,
                &selected,
                receipt_dir,
                cloud::system_now_ms(),
                review_decision.as_ref(),
            )?
        } else {
            cloud_transfer::prepare_cloud_copy_with_review(
                candidate,
                &selected,
                receipt_dir,
                cloud::system_now_ms(),
                review_decision.as_ref(),
            )?
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&CopyOutput {
                action: if adopt_existing {
                    "adopt-existing-copy"
                } else {
                    "copy-only"
                },
                receipt,
                receipt_path: receipt_path.to_string_lossy().into_owned(),
            })
            .map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if args.decision_summary {
        let mut summary = if let Some(output_path) = &args.private_full_review_output {
            let dossier = private_full_review_dossier(&report);
            let (sha256, bytes) = write_private_review_dossier(output_path, &dossier)?;
            private_full_review_summary(&report, &sha256, bytes)
        } else {
            match args.review_reason_set.as_deref() {
                Some(reasons) => review_batch_summary(&report, reasons)?,
                None => decision_summary(&report),
            }
        };
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
    fn parses_defaults_and_explicit_values() {
        let defaults = parse_args(&[], Path::new("/home/test")).unwrap();
        assert_eq!(defaults.root, PathBuf::from("/home/test"));
        assert_eq!(defaults.min_size_mib, 256);
        assert!(defaults.copy_fingerprint.is_none());
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
        assert!(defaults
            .export_naruon_duplicate_canonical_review
            .is_none());
        assert!(defaults.duplicate_canonical_review_output.is_none());
        assert!(defaults
            .duplicate_canonical_review_dossier_output
            .is_none());
        assert!(!defaults.export_semantic_catalog);
        assert!(defaults.naruon_sync_evidence.is_none());
        assert!(!defaults.verify_capacity);
        assert!(!defaults.decision_summary);
        assert!(!defaults.capacity_readiness_summary);
        assert!(!defaults.all_readable_roots);
        assert!(defaults.review_reason_set.is_none());
        assert!(defaults.private_review_output.is_none());
        assert!(defaults.private_full_review_output.is_none());
        assert!(defaults.exact_duplicate_review_prefix.is_none());
        assert!(defaults.exact_duplicate_kind.is_none());
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

        let canonical_review = parse_args(
            &[
                "--export-naruon-duplicate-canonical-review".into(),
                "/audit-lineage.json".into(),
                "--duplicate-canonical-review-output".into(),
                "/canonical-review.json".into(),
                "--duplicate-canonical-review-dossier-output".into(),
                "/canonical-review-dossier.json".into(),
            ],
            Path::new("/home/test"),
        )
        .unwrap();
        assert_eq!(
            canonical_review.export_naruon_duplicate_canonical_review,
            Some(PathBuf::from("/audit-lineage.json"))
        );
        assert_eq!(
            canonical_review.duplicate_canonical_review_output,
            Some(PathBuf::from("/canonical-review.json"))
        );
        assert_eq!(
            canonical_review.duplicate_canonical_review_dossier_output,
            Some(PathBuf::from("/canonical-review-dossier.json"))
        );
        assert!(validate_action_args(&canonical_review).is_ok());
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

        let public_capacity = parse_args(
            &[
                "--all-readable-roots".into(),
                "--decision-summary".into(),
                "--verify-capacity".into(),
                "--capacity-readiness-summary".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(public_capacity.capacity_readiness_summary);
        assert!(validate_action_args(&public_capacity).is_ok());

        for missing_dependencies in [
            vec!["--capacity-readiness-summary".into()],
            vec![
                "--capacity-readiness-summary".into(),
                "--all-readable-roots".into(),
                "--decision-summary".into(),
            ],
        ] {
            let invalid = parse_args(&missing_dependencies, Path::new("/h")).unwrap();
            assert!(validate_action_args(&invalid).is_err());
        }
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
        let private_full_review = parse_args(
            &[
                "--decision-summary".into(),
                "--private-full-review-output".into(),
                "/private/full-review.json".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&private_full_review).is_ok());
        let mut relative_private_full_review = private_full_review.clone();
        relative_private_full_review.private_full_review_output =
            Some(PathBuf::from("full-review.json"));
        assert!(validate_action_args(&relative_private_full_review).is_err());
        let mut reason_scoped_private_full_review = private_full_review.clone();
        reason_scoped_private_full_review.review_reason_set =
            Some(vec!["destination-account-scope-unknown".into()]);
        assert!(validate_action_args(&reason_scoped_private_full_review).is_err());
        let mut multicloud_private_full_review = private_full_review;
        multicloud_private_full_review.all_readable_roots = true;
        assert!(validate_action_args(&multicloud_private_full_review).is_err());
        let dossier_without_export = parse_args(
            &[
                "--duplicate-canonical-review-dossier-output".into(),
                "/private/dossier.json".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&dossier_without_export).is_err());
        let relative_dossier = parse_args(
            &[
                "--export-naruon-duplicate-canonical-review".into(),
                "/audit-lineage.json".into(),
                "--duplicate-canonical-review-dossier-output".into(),
                "dossier.json".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&relative_dossier).is_err());
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
    fn decision_summary_keeps_review_evidence_and_redacts_sensitive_values() {
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
            notices: vec!["dry-run-only".into()],
        };

        let summary = decision_summary(&report);
        let item = &summary["decisions"][0];
        assert_eq!(summary["output_mode"], "decision-summary");
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
        assert_eq!(item["relative_path"], "report.pdf");
        assert_eq!(item["decision_state"], "review-required");
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

        let public_capacity = capacity_readiness_destination_summary(&report);
        assert_eq!(
            public_capacity["output_mode"],
            "capacity-readiness-destination-summary"
        );
        assert_eq!(public_capacity["paths_redacted"], true);
        assert_eq!(public_capacity["relative_names_redacted"], true);
        assert!(public_capacity.get("decisions").is_none());
        assert!(public_capacity.get("decision_batch_fingerprint").is_none());
        assert!(public_capacity
            .get("decision_batch_fingerprint_version")
            .is_none());
        assert!(!has_verified_capacity(&report));

        let encoded = serde_json::to_string(&summary).unwrap();
        for redacted in [
            "/Users/private",
            "private@example.com",
            "Confidential title",
            "Private Author",
            "private raw value",
        ] {
            assert!(!encoded.contains(redacted));
        }

        let reason_set = report.candidates[0].review_reasons.clone();
        let review_batch = review_batch_summary(&report, &reason_set).unwrap();
        assert_eq!(review_batch["output_mode"], "review-batch-summary");
        assert_eq!(review_batch["candidate_count"], 1);
        assert_eq!(review_batch["candidate_bytes"], 42);
        assert_eq!(review_batch["reason_set"], serde_json::json!(reason_set));
        assert_eq!(review_batch["decisions"][0]["relative_path"], "report.pdf");
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

        let mut full_report = report.clone();
        let mut blocked_candidate = full_report.candidates[0].clone();
        blocked_candidate.metadata_fingerprint = "c".repeat(64);
        blocked_candidate.review_fingerprint = "d".repeat(64);
        blocked_candidate.bytes = 7;
        blocked_candidate.blocked_reason = Some("destination-exists".into());
        full_report.candidates.push(blocked_candidate);
        full_report.candidate_bytes += 7;

        let full_dossier = private_full_review_dossier(&full_report);
        assert_eq!(
            full_dossier["output_mode"],
            "private-full-review-dossier"
        );
        assert_eq!(full_dossier["candidate_count"], 2);
        assert_eq!(full_dossier["review_required_count"], 1);
        assert_eq!(full_dossier["blocked_count"], 1);
        assert_eq!(
            full_dossier["metadata_policy"]["blocked_candidates_are_not_approval_eligible"],
            true
        );
        let encoded_full_dossier = serde_json::to_string(&full_dossier).unwrap();
        for private_value in [
            "/Users/private",
            "private@example.com",
            "Confidential title",
            "Private Author",
            "private raw value",
        ] {
            assert!(encoded_full_dossier.contains(private_value));
        }

        let full_summary =
            private_full_review_summary(&full_report, &"f".repeat(64), 123);
        assert_eq!(
            full_summary["output_mode"],
            "private-full-review-summary"
        );
        assert_eq!(full_summary["paths_redacted"], true);
        assert_eq!(full_summary["relative_names_redacted"], true);
        assert_eq!(
            full_summary["private_full_review_dossier"]["checksum_is_not_signature"],
            true
        );
        let encoded_full_summary = serde_json::to_string(&full_summary).unwrap();
        for private_value in [
            "/Users/private",
            "report.pdf",
            "private@example.com",
            "Confidential title",
            "Private Author",
            "private raw value",
        ] {
            assert!(!encoded_full_summary.contains(private_value));
        }

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
        assert_eq!(aggregates["archive_kind"]["counts"]["document"], 3);
        assert_eq!(
            aggregates["archive_kind"]["candidate_bytes"]["document"],
            59
        );
        assert_eq!(aggregates["production_month"]["counts"]["1970-01"], 3);
        assert_eq!(
            aggregates["production_month"]["candidate_bytes"]["1970-01"],
            59
        );
        assert_eq!(
            aggregates["destination_bucket"]["counts"]["1970/01/document"],
            3
        );
        assert_eq!(
            aggregates["destination_bucket"]["candidate_bytes"]["1970/01/document"],
            59
        );
        assert_eq!(
            aggregates["destination_bucket"]["source_relative_path_redacted"],
            true
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
        assert!(help.contains("--export-naruon-copy-readiness --verify-capacity"));
        assert!(help.contains("--naruon-copy-readiness-output ABSOLUTE_NEW_FILE.json"));

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
        args.reviewed_by = None;
        args.review_rationale = None;
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

        let missing_capacity = parse_args(
            &["--export-naruon-copy-readiness".into()],
            Path::new("/h"),
        )
        .unwrap();
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
