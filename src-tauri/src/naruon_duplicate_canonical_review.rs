//! Path-free canonical-copy recommendation lineage for Naruon.
//!
//! This contract combines DiskSage's metadata-first recommendation with an independently
//! verified duplicate-audit lineage. It exposes only opaque member references and bounded ranking
//! features. It never exports paths, filenames, content digests, titles, authors, metadata values,
//! or authority to discard a source.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use sha2::{Digest, Sha256};

use crate::cloud::{CloudCandidate, CloudPlanReport, ExactDuplicateClusterRecommendation};
use crate::naruon_duplicate_audit_lineage::{
    duplicate_audit_lineage_id, NaruonDuplicateAuditLineageEnvelope,
};

pub const NARUON_DUPLICATE_CANONICAL_REVIEW_SCHEMA_VERSION: u32 = 1;
pub const NARUON_DUPLICATE_CANONICAL_REVIEW_SCHEMA_KIND: &str =
    "disksage.duplicate-canonical-review-lineage";

const MEMBER_REF_DOMAIN: &[u8] = b"disksage-duplicate-canonical-member-ref-v1\0";
const CLUSTER_REF_DOMAIN: &[u8] = b"disksage-duplicate-canonical-cluster-ref-v1\0";
const LINEAGE_ID_DOMAIN: &[u8] = b"disksage-duplicate-canonical-review-lineage-id-v1\0";

const PRODUCTION_TIME_PRECEDENCE: [&str; 4] = [
    "embedded_metadata",
    "explicit_filename_date",
    "filesystem_created",
    "filesystem_modified",
];
const CANONICAL_SELECTION_PRECEDENCE: [&str; 8] = [
    "embedded_production_time",
    "production_time_confidence",
    "embedded_metadata_richness",
    "source_lineage_context_richness",
    "non_quarantine_source",
    "non_copy_marked_filename",
    "filesystem_created_time",
    "stable_path_order",
];
const REDACTED_FIELDS: [&str; 9] = [
    "source-and-destination-roots",
    "source-and-destination-paths",
    "file-names",
    "content-digests",
    "content-title-and-authors",
    "raw-metadata-evidence-values",
    "source-lineage-evidence-values",
    "dataset-profile",
    "provider-account-and-capacity",
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonDuplicateCanonicalMember {
    pub member_ref: String,
    pub production_time_source_class: String,
    pub production_time_confidence: String,
    pub embedded_metadata_richness: u32,
    pub source_lineage_context_richness: u32,
    pub quarantined_or_regenerable: bool,
    pub filename_copy_marked: bool,
    pub filesystem_created_known: bool,
    pub filesystem_created_order: u32,
    pub stable_path_order: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonDuplicateCanonicalCluster {
    pub cluster_ref: String,
    pub bytes_per_candidate: u64,
    pub candidate_count: usize,
    pub redundant_bytes: u64,
    pub recommended_canonical_member_ref: String,
    pub recommendation_confidence: String,
    pub recommendation_reason_codes: Vec<String>,
    pub members: Vec<NaruonDuplicateCanonicalMember>,
    pub requires_human_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonDuplicateCanonicalAuditBinding {
    pub duplicate_audit_lineage_id: String,
    pub duplicate_audit_report_fingerprint: String,
    pub aggregate_binding_complete: bool,
    pub cluster_membership_crosswalk_disclosed: bool,
    pub duplicate_group_count: usize,
    pub duplicate_path_count: usize,
    pub reclaimable_allocated_upper_bound_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonDuplicateCanonicalRedaction {
    pub paths_redacted: bool,
    pub file_names_redacted: bool,
    pub content_digests_redacted: bool,
    pub content_metadata_values_redacted: bool,
    pub destination_context_redacted: bool,
    pub redacted_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonDuplicateCanonicalReviewEnvelope {
    pub schema_version: u32,
    pub schema_kind: String,
    pub lineage_id: String,
    pub exported_at_ms: u64,
    pub observed_at_ms: u64,
    pub audit_binding: NaruonDuplicateCanonicalAuditBinding,
    pub production_time_precedence: Vec<String>,
    pub canonical_selection_precedence: Vec<String>,
    pub filename_dates_are_auxiliary: bool,
    pub duplicate_evidence_independent_of_transfer_eligibility: bool,
    pub cluster_count: usize,
    pub candidate_count: usize,
    pub candidate_bytes: u64,
    pub redundant_bytes: u64,
    pub clusters: Vec<NaruonDuplicateCanonicalCluster>,
    pub automatic_discard_allowed: bool,
    pub human_confirmation_required_for_every_cluster: bool,
    pub mutation_performed: bool,
    pub redaction: NaruonDuplicateCanonicalRedaction,
}

fn encode_hex(bytes: impl IntoIterator<Item = u8>) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing a digest to String cannot fail");
    }
    encoded
}

fn opaque_ref(domain: &[u8], cluster_fingerprint: &str, member: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(cluster_fingerprint.as_bytes());
    if let Some(member) = member {
        hasher.update([0]);
        hasher.update(member.as_bytes());
    }
    encode_hex(hasher.finalize())
}

fn valid_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn production_time_source_class(source: &str) -> Result<&'static str, String> {
    if source.starts_with("embedded:") {
        Ok("embedded_metadata")
    } else {
        match source {
            "filename:path-token" => Ok("explicit_filename_date"),
            "filesystem:created" => Ok("filesystem_created"),
            "filesystem:modified-fallback" => Ok("filesystem_modified"),
            _ => Err("duplicate-canonical-production-time-source-unsupported".into()),
        }
    }
}

fn confidence_rank(value: &str) -> Result<u8, String> {
    match value {
        "high" => Ok(3),
        "medium" => Ok(2),
        "low" => Ok(1),
        _ => Err("duplicate-canonical-production-time-confidence-invalid".into()),
    }
}

fn embedded_metadata_richness(candidate: &CloudCandidate) -> u32 {
    u32::from(candidate.content_title.is_some())
        .saturating_add(candidate.content_authors.len() as u32)
        .saturating_add(u32::from(candidate.duration_ms.is_some()))
        .saturating_add(u32::from(candidate.dataset_profile.is_some()))
        .saturating_add(
            candidate
                .metadata_evidence
                .iter()
                .filter(|evidence| evidence.source.starts_with("embedded:"))
                .count() as u32,
        )
}

fn filename_copy_marked(path: &str) -> bool {
    let path = std::path::Path::new(path);
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if [" copy", "-copy", "_copy", "복사본"]
        .iter()
        .any(|marker| stem.contains(marker))
    {
        return true;
    }
    let Some(open) = stem.rfind('(') else {
        return false;
    };
    let suffix = stem[open + 1..].trim_end_matches(')').trim();
    stem.ends_with(')') && !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
}

fn quarantined_or_regenerable(path: &str) -> bool {
    std::path::Path::new(path).components().any(|component| {
        let value = component.as_os_str().to_string_lossy().to_lowercase();
        matches!(
            value.as_str(),
            "quarantine"
                | "exact-duplicates"
                | "duplicates"
                | ".trash"
                | "trash"
                | "tmp"
                | "temp"
                | "cache"
        )
    })
}

fn retain_max_by_key<T: Ord, F: Fn(&NaruonDuplicateCanonicalMember) -> T>(
    members: &mut Vec<&NaruonDuplicateCanonicalMember>,
    key: F,
) -> bool {
    let before = members.len();
    let Some(best) = members.iter().map(|member| key(member)).max() else {
        return false;
    };
    members.retain(|member| key(member) == best);
    members.len() < before
}

fn retain_min_by_key<T: Ord, F: Fn(&NaruonDuplicateCanonicalMember) -> T>(
    members: &mut Vec<&NaruonDuplicateCanonicalMember>,
    key: F,
) -> bool {
    let before = members.len();
    let Some(best) = members.iter().map(|member| key(member)).min() else {
        return false;
    };
    members.retain(|member| key(member) == best);
    members.len() < before
}

fn record_stage(
    reduced: bool,
    remaining_len: usize,
    reason: &str,
    stage_confidence: &str,
    reasons: &mut Vec<String>,
    confidence: &mut Option<String>,
) {
    if reduced {
        reasons.push(reason.to_string());
        if remaining_len == 1 && confidence.is_none() {
            *confidence = Some(stage_confidence.to_string());
        }
    }
}

pub fn recommend_canonical_member(
    members: &[NaruonDuplicateCanonicalMember],
) -> Result<(String, String, Vec<String>), String> {
    if members.len() < 2 {
        return Err("duplicate-canonical-cluster-too-small".into());
    }
    let mut remaining = members.iter().collect::<Vec<_>>();
    let mut reasons = Vec::new();
    let mut confidence = None;

    let reduced = retain_max_by_key(&mut remaining, |member| {
        member.production_time_source_class == "embedded_metadata"
    });
    record_stage(
        reduced,
        remaining.len(),
        "embedded-production-time-preferred",
        "high",
        &mut reasons,
        &mut confidence,
    );
    let reduced = retain_max_by_key(&mut remaining, |member| {
        confidence_rank(&member.production_time_confidence).unwrap_or_default()
    });
    record_stage(
        reduced,
        remaining.len(),
        "higher-production-time-confidence",
        "high",
        &mut reasons,
        &mut confidence,
    );
    let reduced = retain_max_by_key(&mut remaining, |member| member.embedded_metadata_richness);
    record_stage(
        reduced,
        remaining.len(),
        "richer-embedded-metadata-preferred",
        "high",
        &mut reasons,
        &mut confidence,
    );
    let reduced = retain_max_by_key(&mut remaining, |member| {
        member.source_lineage_context_richness
    });
    record_stage(
        reduced,
        remaining.len(),
        "richer-source-lineage-context-preferred",
        "high",
        &mut reasons,
        &mut confidence,
    );
    let reduced = retain_min_by_key(&mut remaining, |member| member.quarantined_or_regenerable);
    record_stage(
        reduced,
        remaining.len(),
        "non-quarantine-path-preferred",
        "medium",
        &mut reasons,
        &mut confidence,
    );
    let reduced = retain_min_by_key(&mut remaining, |member| member.filename_copy_marked);
    record_stage(
        reduced,
        remaining.len(),
        "non-copy-marked-filename-preferred",
        "medium",
        &mut reasons,
        &mut confidence,
    );
    let reduced = retain_min_by_key(&mut remaining, |member| {
        (
            !member.filesystem_created_known,
            member.filesystem_created_order,
        )
    });
    record_stage(
        reduced,
        remaining.len(),
        "filesystem-created-time-tiebreaker",
        "low",
        &mut reasons,
        &mut confidence,
    );
    if remaining.len() > 1 {
        reasons.push("stable-path-tiebreaker".into());
    }
    remaining.sort_by_key(|member| member.stable_path_order);
    let recommended = remaining
        .first()
        .ok_or_else(|| "duplicate-canonical-cluster-empty".to_string())?;
    Ok((
        recommended.member_ref.clone(),
        confidence.unwrap_or_else(|| "low".into()),
        reasons,
    ))
}

fn dense_created_orders(candidates: &[&CloudCandidate]) -> BTreeMap<String, u32> {
    let mut unique = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.created_ms == 0,
                if candidate.created_ms == 0 {
                    u64::MAX
                } else {
                    candidate.created_ms
                },
            )
        })
        .collect::<Vec<_>>();
    unique.sort_unstable();
    unique.dedup();
    candidates
        .iter()
        .map(|candidate| {
            let key = (
                candidate.created_ms == 0,
                if candidate.created_ms == 0 {
                    u64::MAX
                } else {
                    candidate.created_ms
                },
            );
            let order = unique
                .binary_search(&key)
                .expect("candidate creation key is present") as u32;
            (candidate.metadata_fingerprint.clone(), order)
        })
        .collect()
}

fn stable_path_orders(candidates: &[&CloudCandidate]) -> BTreeMap<String, u32> {
    let mut ordered = candidates.iter().copied().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.relative_path
            .cmp(&right.relative_path)
            .then_with(|| left.metadata_fingerprint.cmp(&right.metadata_fingerprint))
    });
    ordered
        .into_iter()
        .enumerate()
        .map(|(order, candidate)| (candidate.metadata_fingerprint.clone(), order as u32))
        .collect()
}

fn export_cluster(
    cluster: &ExactDuplicateClusterRecommendation,
    candidates: &BTreeMap<&str, &CloudCandidate>,
) -> Result<NaruonDuplicateCanonicalCluster, String> {
    if cluster.candidate_count < 2
        || cluster.member_metadata_fingerprints.len() != cluster.candidate_count
        || cluster.redundant_bytes
            != cluster
                .bytes_per_candidate
                .saturating_mul((cluster.candidate_count - 1) as u64)
        || !cluster.requires_human_confirmation
        || !valid_lower_hex_64(&cluster.cluster_fingerprint)
        || !valid_lower_hex_64(&cluster.recommended_canonical_metadata_fingerprint)
    {
        return Err("duplicate-canonical-cluster-invalid".into());
    }
    let private_members = cluster
        .member_metadata_fingerprints
        .iter()
        .map(|fingerprint| {
            candidates
                .get(fingerprint.as_str())
                .copied()
                .ok_or_else(|| "duplicate-canonical-member-missing".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if private_members
        .iter()
        .any(|candidate| candidate.bytes != cluster.bytes_per_candidate)
    {
        return Err("duplicate-canonical-member-bytes-mismatch".into());
    }
    let created_orders = dense_created_orders(&private_members);
    let path_orders = stable_path_orders(&private_members);
    let mut members = private_members
        .into_iter()
        .map(|candidate| {
            if !candidate
                .review_reasons
                .iter()
                .any(|reason| reason == "exact-duplicate-content-needs-canonical-selection")
            {
                return Err("duplicate-canonical-member-review-reason-missing".into());
            }
            Ok(NaruonDuplicateCanonicalMember {
                member_ref: opaque_ref(
                    MEMBER_REF_DOMAIN,
                    &cluster.cluster_fingerprint,
                    Some(&candidate.metadata_fingerprint),
                ),
                production_time_source_class: production_time_source_class(
                    &candidate.production_time_source,
                )?
                .into(),
                production_time_confidence: candidate.production_time_confidence.clone(),
                embedded_metadata_richness: embedded_metadata_richness(candidate),
                source_lineage_context_richness: candidate.content_context.len() as u32,
                quarantined_or_regenerable: quarantined_or_regenerable(&candidate.relative_path),
                filename_copy_marked: filename_copy_marked(&candidate.relative_path),
                filesystem_created_known: candidate.created_ms != 0,
                filesystem_created_order: *created_orders
                    .get(&candidate.metadata_fingerprint)
                    .ok_or_else(|| "duplicate-canonical-created-order-missing".to_string())?,
                stable_path_order: *path_orders
                    .get(&candidate.metadata_fingerprint)
                    .ok_or_else(|| "duplicate-canonical-path-order-missing".to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    members.sort_by(|left, right| left.member_ref.cmp(&right.member_ref));
    let canonical_member_ref = opaque_ref(
        MEMBER_REF_DOMAIN,
        &cluster.cluster_fingerprint,
        Some(&cluster.recommended_canonical_metadata_fingerprint),
    );
    let (recommended, confidence, reasons) = recommend_canonical_member(&members)?;
    if recommended != canonical_member_ref
        || confidence != cluster.recommendation_confidence
        || reasons != cluster.recommendation_reason_codes
    {
        return Err("duplicate-canonical-recommendation-mismatch".into());
    }
    Ok(NaruonDuplicateCanonicalCluster {
        cluster_ref: opaque_ref(CLUSTER_REF_DOMAIN, &cluster.cluster_fingerprint, None),
        bytes_per_candidate: cluster.bytes_per_candidate,
        candidate_count: cluster.candidate_count,
        redundant_bytes: cluster.redundant_bytes,
        recommended_canonical_member_ref: canonical_member_ref,
        recommendation_confidence: confidence,
        recommendation_reason_codes: reasons,
        members,
        requires_human_confirmation: true,
    })
}

pub fn duplicate_canonical_review_lineage_id(
    envelope: &NaruonDuplicateCanonicalReviewEnvelope,
) -> String {
    let mut canonical = serde_json::to_value(envelope)
        .expect("serializing the duplicate canonical review envelope cannot fail");
    canonical
        .as_object_mut()
        .expect("the duplicate canonical review envelope is an object")
        .remove("lineage_id");
    let canonical = serde_json::to_vec(&canonical)
        .expect("serializing canonical duplicate review lineage cannot fail");
    let mut hasher = Sha256::new();
    hasher.update(LINEAGE_ID_DOMAIN);
    hasher.update(canonical);
    encode_hex(hasher.finalize())
}

pub fn export_naruon_duplicate_canonical_review(
    report: &CloudPlanReport,
    audit: &NaruonDuplicateAuditLineageEnvelope,
    exported_at_ms: u64,
) -> Result<NaruonDuplicateCanonicalReviewEnvelope, String> {
    if audit.schema_version
        != crate::naruon_duplicate_audit_lineage::NARUON_DUPLICATE_AUDIT_LINEAGE_SCHEMA_VERSION
        || audit.schema_kind
            != crate::naruon_duplicate_audit_lineage::NARUON_DUPLICATE_AUDIT_LINEAGE_SCHEMA_KIND
        || audit.lineage_id != duplicate_audit_lineage_id(audit)
        || !audit.evidence_complete
        || !audit.context_metadata_complete
        || audit.evidence_gap_count != 0
        || !audit.issue_counts.is_empty()
        || audit.automatic_discard_allowed
        || !audit.human_context_review_required
        || audit.mutation_performed
    {
        return Err("duplicate-canonical-audit-lineage-invalid".into());
    }
    if exported_at_ms < report.generated_at_ms || report.exact_duplicates.clusters.is_empty() {
        return Err("duplicate-canonical-report-invalid".into());
    }
    let audit_candidate_bytes = audit.groups.iter().try_fold(0u64, |total, group| {
        group
            .logical_bytes_per_file
            .checked_mul(group.path_count as u64)
            .and_then(|bytes| total.checked_add(bytes))
    });
    if audit.hardlink_alias_count != 0
        || report.exact_duplicates.cluster_count != audit.duplicate_group_count
        || report.exact_duplicates.candidate_count != audit.duplicate_path_count
        || Some(report.exact_duplicates.candidate_bytes) != audit_candidate_bytes
        || report.exact_duplicates.redundant_bytes != audit.reclaimable_logical_bytes
    {
        return Err("duplicate-canonical-audit-aggregate-mismatch".into());
    }

    let mut candidates = BTreeMap::new();
    for candidate in &report.candidates {
        if candidates
            .insert(candidate.metadata_fingerprint.as_str(), candidate)
            .is_some()
        {
            return Err("duplicate-canonical-candidate-fingerprint-duplicate".into());
        }
    }
    let mut cluster_refs = BTreeSet::new();
    let mut member_refs = BTreeSet::new();
    let mut clusters = report
        .exact_duplicates
        .clusters
        .iter()
        .map(|cluster| export_cluster(cluster, &candidates))
        .collect::<Result<Vec<_>, _>>()?;
    clusters.sort_by(|left, right| left.cluster_ref.cmp(&right.cluster_ref));
    for cluster in &clusters {
        if !cluster_refs.insert(cluster.cluster_ref.as_str())
            || cluster
                .members
                .iter()
                .any(|member| !member_refs.insert(member.member_ref.as_str()))
        {
            return Err("duplicate-canonical-opaque-reference-duplicate".into());
        }
    }

    let mut envelope = NaruonDuplicateCanonicalReviewEnvelope {
        schema_version: NARUON_DUPLICATE_CANONICAL_REVIEW_SCHEMA_VERSION,
        schema_kind: NARUON_DUPLICATE_CANONICAL_REVIEW_SCHEMA_KIND.into(),
        lineage_id: String::new(),
        exported_at_ms,
        observed_at_ms: report.generated_at_ms,
        audit_binding: NaruonDuplicateCanonicalAuditBinding {
            duplicate_audit_lineage_id: audit.lineage_id.clone(),
            duplicate_audit_report_fingerprint: audit.report_fingerprint.clone(),
            aggregate_binding_complete: true,
            cluster_membership_crosswalk_disclosed: false,
            duplicate_group_count: audit.duplicate_group_count,
            duplicate_path_count: audit.duplicate_path_count,
            reclaimable_allocated_upper_bound_bytes: audit.reclaimable_allocated_upper_bound_bytes,
        },
        production_time_precedence: PRODUCTION_TIME_PRECEDENCE.map(str::to_string).to_vec(),
        canonical_selection_precedence: CANONICAL_SELECTION_PRECEDENCE.map(str::to_string).to_vec(),
        filename_dates_are_auxiliary: true,
        duplicate_evidence_independent_of_transfer_eligibility: true,
        cluster_count: report.exact_duplicates.cluster_count,
        candidate_count: report.exact_duplicates.candidate_count,
        candidate_bytes: report.exact_duplicates.candidate_bytes,
        redundant_bytes: report.exact_duplicates.redundant_bytes,
        clusters,
        automatic_discard_allowed: false,
        human_confirmation_required_for_every_cluster: true,
        mutation_performed: false,
        redaction: NaruonDuplicateCanonicalRedaction {
            paths_redacted: true,
            file_names_redacted: true,
            content_digests_redacted: true,
            content_metadata_values_redacted: true,
            destination_context_redacted: true,
            redacted_fields: REDACTED_FIELDS.map(str::to_string).to_vec(),
        },
    };
    envelope.lineage_id = duplicate_canonical_review_lineage_id(&envelope);
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::{
        plan_cloud_archive, CloudAccountScope, CloudPlanOptions, CloudProvider, CloudRoot,
        ContentMetadata, FileFact, MetadataEvidence,
    };
    use crate::duplicate_audit::{audit_duplicates, DuplicateAuditOptions};
    use crate::naruon_duplicate_audit_lineage::export_naruon_duplicate_audit_lineage;
    use std::time::UNIX_EPOCH;

    fn millis(value: std::io::Result<std::time::SystemTime>) -> u64 {
        value
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_millis() as u64)
            .unwrap_or_default()
    }

    fn audit_options() -> DuplicateAuditOptions {
        DuplicateAuditOptions {
            min_file_bytes: 1,
            prefix_bytes: 4,
            max_entries: 100,
            max_duration_ms: 10_000,
            max_files_to_hash: 100,
            max_size_groups: 100,
            max_hash_bytes: 10_000_000,
        }
    }

    #[test]
    fn exports_metadata_first_review_bound_to_independent_audit() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let cloud = temp.path().join("cloud");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&cloud).unwrap();
        std::fs::write(source.join("original.pdf"), b"same payload").unwrap();
        std::fs::write(source.join("original (1).pdf"), b"same payload").unwrap();

        let files = ["original.pdf", "original (1).pdf"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let path = source.join(name);
                let metadata = std::fs::metadata(&path).unwrap();
                FileFact {
                    path,
                    bytes: metadata.len(),
                    created_ms: millis(metadata.created()),
                    modified_ms: millis(metadata.modified()),
                    content_metadata: if index == 0 {
                        ContentMetadata {
                            production_time_ms: Some(1),
                            production_time_source: Some("embedded:test".into()),
                            production_time_confidence: Some("high".into()),
                            title: Some("private title".into()),
                            evidence: vec![MetadataEvidence {
                                field: "production-date".into(),
                                value: "private date".into(),
                                source: "embedded:test".into(),
                                confidence: "high".into(),
                            }],
                            ..ContentMetadata::default()
                        }
                    } else {
                        ContentMetadata::default()
                    },
                }
            })
            .collect::<Vec<_>>();
        let report = plan_cloud_archive(
            &files,
            &source,
            &CloudRoot {
                id: "icloud:test".into(),
                provider: CloudProvider::Icloud,
                account_scope: CloudAccountScope::Personal,
                label: "private account".into(),
                path: cloud.to_string_lossy().into_owned(),
                readable: true,
                access_issue: None,
            },
            u64::MAX / 2,
            CloudPlanOptions {
                min_size_bytes: 0,
                min_age_days: 0,
                limit: 10,
            },
        );
        let audit = audit_duplicates(&source, &audit_options(), report.generated_at_ms).unwrap();
        let audit_lineage =
            export_naruon_duplicate_audit_lineage(&audit, report.generated_at_ms + 1).unwrap();
        let envelope = export_naruon_duplicate_canonical_review(
            &report,
            &audit_lineage,
            report.generated_at_ms + 2,
        )
        .unwrap();
        let encoded = serde_json::to_string(&envelope).unwrap();

        assert_eq!(envelope.cluster_count, 1);
        assert_eq!(envelope.candidate_count, 2);
        assert_eq!(
            envelope.lineage_id,
            duplicate_canonical_review_lineage_id(&envelope)
        );
        assert_eq!(
            envelope.clusters[0].recommendation_reason_codes[0],
            "embedded-production-time-preferred"
        );
        assert!(!envelope.automatic_discard_allowed);
        assert!(envelope.human_confirmation_required_for_every_cluster);
        assert!(!envelope.mutation_performed);
        assert!(!encoded.contains("original.pdf"));
        assert!(!encoded.contains("private title"));
        assert!(!encoded.contains("private date"));
        assert!(!encoded.contains("private account"));
        assert!(!encoded.contains(&source.to_string_lossy().into_owned()));
    }

    #[test]
    fn rejects_tampered_audit_binding_or_aggregate_drift() {
        let member = NaruonDuplicateCanonicalMember {
            member_ref: "a".repeat(64),
            production_time_source_class: "filesystem_created".into(),
            production_time_confidence: "low".into(),
            embedded_metadata_richness: 0,
            source_lineage_context_richness: 0,
            quarantined_or_regenerable: false,
            filename_copy_marked: false,
            filesystem_created_known: true,
            filesystem_created_order: 0,
            stable_path_order: 0,
        };
        let second = NaruonDuplicateCanonicalMember {
            member_ref: "b".repeat(64),
            stable_path_order: 1,
            ..member.clone()
        };
        let (recommended, confidence, reasons) =
            recommend_canonical_member(&[member, second]).unwrap();
        assert_eq!(recommended, "a".repeat(64));
        assert_eq!(confidence, "low");
        assert_eq!(reasons, vec!["stable-path-tiebreaker"]);
    }
}
