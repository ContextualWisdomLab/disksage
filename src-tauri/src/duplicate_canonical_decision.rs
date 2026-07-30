//! Local human decisions for exact-duplicate canonical selection.
//!
//! A decision binds a human-selected canonical member to the secure local review dossier. It is
//! deliberately not an authorization to discard any alternative and never mutates source files.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cloud_review::validate_review_attribution;
use crate::naruon_duplicate_canonical_review::{
    local_duplicate_canonical_review_dossier_id, LocalDuplicateCanonicalReviewCluster,
    LocalDuplicateCanonicalReviewDossier, LocalDuplicateCanonicalReviewMember,
    LOCAL_DUPLICATE_CANONICAL_REVIEW_DOSSIER_SCHEMA_KIND,
    LOCAL_DUPLICATE_CANONICAL_REVIEW_DOSSIER_SCHEMA_VERSION,
};

pub const LOCAL_DUPLICATE_CANONICAL_DECISION_SCHEMA_VERSION: u32 = 1;
pub const LOCAL_DUPLICATE_CANONICAL_DECISION_SCHEMA_KIND: &str =
    "disksage.local-duplicate-canonical-selection-decision";
pub const LOCAL_DUPLICATE_CANONICAL_VERIFICATION_SCHEMA_VERSION: u32 = 1;

const DECISION_ID_DOMAIN: &[u8] = b"disksage-local-duplicate-canonical-selection-decision-id-v1\0";
const MAX_DECISION_BYTES: usize = 64 * 1024;
const PRODUCTION_TIME_PRECEDENCE: [&str; 4] = [
    "embedded_metadata",
    "explicit_filename_date",
    "filesystem_created",
    "filesystem_modified",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DuplicateCanonicalDecisionDisposition {
    Selected,
    Held,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalDuplicateCanonicalDecision {
    pub schema_version: u32,
    pub schema_kind: String,
    pub decision_id: String,
    pub dossier_id: String,
    pub canonical_review_lineage_id: String,
    pub duplicate_audit_lineage_id: String,
    pub cluster_ref: String,
    pub recommendation_confidence: String,
    pub recommended_canonical_member_ref: String,
    pub reviewed_member_refs: Vec<String>,
    pub disposition: DuplicateCanonicalDecisionDisposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_canonical_member_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_matches_recommendation: Option<bool>,
    pub reviewed_at_ms: u64,
    pub reviewed_by: String,
    pub rationale: String,
    pub source_stability_revalidated: bool,
    pub canonical_selection_recorded: bool,
    pub discard_authorization: bool,
    pub mutation_performed: bool,
    pub cloud_write_performed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateCanonicalConfidenceSummary {
    pub clusters: usize,
    pub candidates: usize,
    pub redundant_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalDuplicateCanonicalVerificationSummary {
    pub schema_version: u32,
    pub output_mode: String,
    pub verified_at_ms: u64,
    pub dossier_id: String,
    pub canonical_review_lineage_id: String,
    pub duplicate_audit_lineage_id: String,
    pub cluster_count: usize,
    pub candidate_count: usize,
    pub candidate_bytes: u64,
    pub redundant_bytes: u64,
    pub recommendation_confidence: BTreeMap<String, DuplicateCanonicalConfidenceSummary>,
    pub filesystem_stable_candidate_count: usize,
    pub filesystem_stability_revalidated: bool,
    pub contains_local_paths: bool,
    pub canonical_decision_created: bool,
    pub discard_authorization: bool,
    pub mutation_performed: bool,
    pub cloud_write_performed: bool,
}

fn encode_hex(bytes: impl IntoIterator<Item = u8>) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing a digest to String cannot fail");
    }
    encoded
}

fn valid_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn relative_path_is_normal(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn validate_member_structure(
    source_root: &Path,
    bytes_per_candidate: u64,
    member: &LocalDuplicateCanonicalReviewMember,
) -> Result<(), String> {
    if !valid_lower_hex_64(&member.member_ref)
        || !valid_lower_hex_64(&member.metadata_fingerprint)
        || !Path::new(&member.absolute_source_path).is_absolute()
        || !relative_path_is_normal(Path::new(&member.relative_path))
        || Path::new(&member.absolute_source_path)
            .strip_prefix(source_root)
            .ok()
            != Some(Path::new(&member.relative_path))
        || member.bytes != bytes_per_candidate
        || !member.filesystem_stable_at_export
    {
        return Err("duplicate-canonical-dossier-member-contract-invalid".into());
    }
    Ok(())
}

fn validate_cluster_structure(
    source_root: &Path,
    cluster: &LocalDuplicateCanonicalReviewCluster,
    global_member_refs: &mut BTreeSet<String>,
) -> Result<(), String> {
    if !valid_lower_hex_64(&cluster.cluster_ref)
        || !matches!(
            cluster.recommendation_confidence.as_str(),
            "high" | "medium" | "low"
        )
        || !cluster.requires_human_confirmation
        || cluster.candidate_count != 1usize.saturating_add(cluster.alternatives.len())
        || cluster.candidate_count < 2
        || cluster.bytes_per_candidate == 0
        || cluster.redundant_bytes
            != cluster
                .bytes_per_candidate
                .saturating_mul((cluster.candidate_count - 1) as u64)
    {
        return Err("duplicate-canonical-dossier-cluster-contract-invalid".into());
    }

    for member in std::iter::once(&cluster.recommended_canonical).chain(&cluster.alternatives) {
        validate_member_structure(source_root, cluster.bytes_per_candidate, member)?;
        if !global_member_refs.insert(member.member_ref.clone()) {
            return Err("duplicate-canonical-dossier-member-ref-duplicate".into());
        }
    }
    Ok(())
}

pub fn validate_local_duplicate_canonical_review_dossier(
    dossier: &LocalDuplicateCanonicalReviewDossier,
) -> Result<(), String> {
    if dossier.schema_version != LOCAL_DUPLICATE_CANONICAL_REVIEW_DOSSIER_SCHEMA_VERSION
        || dossier.schema_kind != LOCAL_DUPLICATE_CANONICAL_REVIEW_DOSSIER_SCHEMA_KIND
        || dossier.dossier_id != local_duplicate_canonical_review_dossier_id(dossier)
        || !valid_lower_hex_64(&dossier.dossier_id)
        || !valid_lower_hex_64(&dossier.canonical_review_lineage_id)
        || !valid_lower_hex_64(&dossier.duplicate_audit_lineage_id)
        || !Path::new(&dossier.source_root).is_absolute()
        || dossier.production_time_precedence
            != PRODUCTION_TIME_PRECEDENCE.map(str::to_string).to_vec()
        || !dossier.filename_dates_are_auxiliary
        || !dossier.local_sensitive_metadata
        || dossier.naruon_submission_allowed
        || dossier.automatic_discard_allowed
        || !dossier.human_confirmation_required_for_every_cluster
        || dossier.mutation_performed
    {
        return Err("duplicate-canonical-dossier-contract-invalid".into());
    }

    let source_root = Path::new(&dossier.source_root);
    let mut cluster_refs = BTreeSet::new();
    let mut member_refs = BTreeSet::new();
    let mut candidate_count = 0usize;
    let mut candidate_bytes = 0u64;
    let mut redundant_bytes = 0u64;
    for cluster in &dossier.clusters {
        if !cluster_refs.insert(cluster.cluster_ref.clone()) {
            return Err("duplicate-canonical-dossier-cluster-ref-duplicate".into());
        }
        validate_cluster_structure(source_root, cluster, &mut member_refs)?;
        candidate_count = candidate_count.saturating_add(cluster.candidate_count);
        candidate_bytes = candidate_bytes.saturating_add(
            cluster
                .bytes_per_candidate
                .saturating_mul(cluster.candidate_count as u64),
        );
        redundant_bytes = redundant_bytes.saturating_add(cluster.redundant_bytes);
    }
    if dossier.cluster_count != dossier.clusters.len()
        || dossier.candidate_count != candidate_count
        || dossier.candidate_bytes != candidate_bytes
        || dossier.redundant_bytes != redundant_bytes
    {
        return Err("duplicate-canonical-dossier-arithmetic-mismatch".into());
    }
    Ok(())
}

fn filesystem_millis(value: std::io::Result<std::time::SystemTime>) -> u64 {
    value
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn revalidate_member(
    canonical_source_root: &Path,
    member: &LocalDuplicateCanonicalReviewMember,
) -> Result<(), String> {
    let path = Path::new(&member.absolute_source_path);
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "duplicate-canonical-decision-source-unavailable".to_string())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != member.bytes
        || filesystem_millis(metadata.modified()) != member.modified_ms
    {
        return Err("duplicate-canonical-decision-source-stability-invalid".into());
    }
    let canonical_path = std::fs::canonicalize(path)
        .map_err(|_| "duplicate-canonical-decision-source-unavailable".to_string())?;
    if !canonical_path.starts_with(canonical_source_root) {
        return Err("duplicate-canonical-decision-source-outside-root".into());
    }
    Ok(())
}

fn revalidate_cluster(
    dossier: &LocalDuplicateCanonicalReviewDossier,
    cluster: &LocalDuplicateCanonicalReviewCluster,
) -> Result<(), String> {
    let source_metadata = std::fs::symlink_metadata(&dossier.source_root)
        .map_err(|_| "duplicate-canonical-decision-source-root-unavailable".to_string())?;
    if !source_metadata.is_dir() || source_metadata.file_type().is_symlink() {
        return Err("duplicate-canonical-decision-source-root-unsafe".into());
    }
    let canonical_source_root = std::fs::canonicalize(&dossier.source_root)
        .map_err(|_| "duplicate-canonical-decision-source-root-unavailable".to_string())?;
    for member in std::iter::once(&cluster.recommended_canonical).chain(&cluster.alternatives) {
        revalidate_member(&canonical_source_root, member)?;
    }
    Ok(())
}

pub fn verify_local_duplicate_canonical_review_dossier(
    dossier: &LocalDuplicateCanonicalReviewDossier,
    verified_at_ms: u64,
) -> Result<LocalDuplicateCanonicalVerificationSummary, String> {
    validate_local_duplicate_canonical_review_dossier(dossier)?;
    if verified_at_ms == 0 {
        return Err("duplicate-canonical-dossier-verification-time-invalid".into());
    }

    let mut confidence = BTreeMap::<String, DuplicateCanonicalConfidenceSummary>::new();
    for cluster in &dossier.clusters {
        revalidate_cluster(dossier, cluster)?;
        let entry = confidence
            .entry(cluster.recommendation_confidence.clone())
            .or_default();
        entry.clusters = entry.clusters.saturating_add(1);
        entry.candidates = entry.candidates.saturating_add(cluster.candidate_count);
        entry.redundant_bytes = entry
            .redundant_bytes
            .saturating_add(cluster.redundant_bytes);
    }

    Ok(LocalDuplicateCanonicalVerificationSummary {
        schema_version: LOCAL_DUPLICATE_CANONICAL_VERIFICATION_SCHEMA_VERSION,
        output_mode: "local-duplicate-canonical-review-dossier-verification".into(),
        verified_at_ms,
        dossier_id: dossier.dossier_id.clone(),
        canonical_review_lineage_id: dossier.canonical_review_lineage_id.clone(),
        duplicate_audit_lineage_id: dossier.duplicate_audit_lineage_id.clone(),
        cluster_count: dossier.cluster_count,
        candidate_count: dossier.candidate_count,
        candidate_bytes: dossier.candidate_bytes,
        redundant_bytes: dossier.redundant_bytes,
        recommendation_confidence: confidence,
        filesystem_stable_candidate_count: dossier.candidate_count,
        filesystem_stability_revalidated: true,
        contains_local_paths: false,
        canonical_decision_created: false,
        discard_authorization: false,
        mutation_performed: false,
        cloud_write_performed: false,
    })
}

fn decision_id(decision: &LocalDuplicateCanonicalDecision) -> String {
    let mut canonical =
        serde_json::to_value(decision).expect("serializing a canonical decision cannot fail");
    canonical
        .as_object_mut()
        .expect("a canonical decision is an object")
        .remove("decision_id");
    let encoded =
        serde_json::to_vec(&canonical).expect("serializing canonical decision JSON cannot fail");
    let mut hasher = Sha256::new();
    hasher.update(DECISION_ID_DOMAIN);
    hasher.update(encoded);
    encode_hex(hasher.finalize())
}

fn cluster_by_ref<'a>(
    dossier: &'a LocalDuplicateCanonicalReviewDossier,
    cluster_ref: &str,
) -> Result<&'a LocalDuplicateCanonicalReviewCluster, String> {
    let matches = dossier
        .clusters
        .iter()
        .filter(|cluster| cluster.cluster_ref == cluster_ref)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [only] => Ok(*only),
        [] => Err("duplicate-canonical-decision-cluster-missing".into()),
        _ => Err("duplicate-canonical-decision-cluster-ambiguous".into()),
    }
}

fn reviewed_member_refs(cluster: &LocalDuplicateCanonicalReviewCluster) -> Vec<String> {
    let mut refs = std::iter::once(&cluster.recommended_canonical)
        .chain(&cluster.alternatives)
        .map(|member| member.member_ref.clone())
        .collect::<Vec<_>>();
    refs.sort();
    refs
}

pub fn create_local_duplicate_canonical_decision(
    dossier: &LocalDuplicateCanonicalReviewDossier,
    cluster_ref: &str,
    disposition: DuplicateCanonicalDecisionDisposition,
    selected_canonical_member_ref: Option<&str>,
    reviewed_at_ms: u64,
    reviewed_by: &str,
    rationale: &str,
) -> Result<LocalDuplicateCanonicalDecision, String> {
    validate_local_duplicate_canonical_review_dossier(dossier)?;
    if !valid_lower_hex_64(cluster_ref) || reviewed_at_ms == 0 {
        return Err("duplicate-canonical-decision-argument-invalid".into());
    }
    validate_review_attribution(reviewed_by, rationale)?;
    let cluster = cluster_by_ref(dossier, cluster_ref)?;
    revalidate_cluster(dossier, cluster)?;
    let member_refs = reviewed_member_refs(cluster);
    let selected = selected_canonical_member_ref.map(str::to_string);
    match disposition {
        DuplicateCanonicalDecisionDisposition::Selected => {
            if selected
                .as_ref()
                .is_none_or(|member_ref| !member_refs.contains(member_ref))
            {
                return Err("duplicate-canonical-decision-selected-member-invalid".into());
            }
        }
        DuplicateCanonicalDecisionDisposition::Held => {
            if selected.is_some() {
                return Err("duplicate-canonical-decision-held-selection-unexpected".into());
            }
        }
    }
    let selection_matches_recommendation = selected
        .as_ref()
        .map(|member_ref| member_ref == &cluster.recommended_canonical.member_ref);
    let mut decision = LocalDuplicateCanonicalDecision {
        schema_version: LOCAL_DUPLICATE_CANONICAL_DECISION_SCHEMA_VERSION,
        schema_kind: LOCAL_DUPLICATE_CANONICAL_DECISION_SCHEMA_KIND.into(),
        decision_id: String::new(),
        dossier_id: dossier.dossier_id.clone(),
        canonical_review_lineage_id: dossier.canonical_review_lineage_id.clone(),
        duplicate_audit_lineage_id: dossier.duplicate_audit_lineage_id.clone(),
        cluster_ref: cluster.cluster_ref.clone(),
        recommendation_confidence: cluster.recommendation_confidence.clone(),
        recommended_canonical_member_ref: cluster.recommended_canonical.member_ref.clone(),
        reviewed_member_refs: member_refs,
        disposition,
        selected_canonical_member_ref: selected,
        selection_matches_recommendation,
        reviewed_at_ms,
        reviewed_by: reviewed_by.into(),
        rationale: rationale.into(),
        source_stability_revalidated: true,
        canonical_selection_recorded: matches!(
            disposition,
            DuplicateCanonicalDecisionDisposition::Selected
        ),
        discard_authorization: false,
        mutation_performed: false,
        cloud_write_performed: false,
    };
    decision.decision_id = decision_id(&decision);
    Ok(decision)
}

pub fn validate_local_duplicate_canonical_decision(
    dossier: &LocalDuplicateCanonicalReviewDossier,
    decision: &LocalDuplicateCanonicalDecision,
) -> Result<(), String> {
    validate_local_duplicate_canonical_review_dossier(dossier)?;
    if decision.schema_version != LOCAL_DUPLICATE_CANONICAL_DECISION_SCHEMA_VERSION
        || decision.schema_kind != LOCAL_DUPLICATE_CANONICAL_DECISION_SCHEMA_KIND
        || !valid_lower_hex_64(&decision.decision_id)
        || decision.decision_id != decision_id(decision)
        || decision.dossier_id != dossier.dossier_id
        || decision.canonical_review_lineage_id != dossier.canonical_review_lineage_id
        || decision.duplicate_audit_lineage_id != dossier.duplicate_audit_lineage_id
        || decision.reviewed_at_ms == 0
        || !decision.source_stability_revalidated
        || decision.discard_authorization
        || decision.mutation_performed
        || decision.cloud_write_performed
    {
        return Err("duplicate-canonical-decision-contract-invalid".into());
    }
    validate_review_attribution(&decision.reviewed_by, &decision.rationale)?;
    let cluster = cluster_by_ref(dossier, &decision.cluster_ref)?;
    revalidate_cluster(dossier, cluster)?;
    let member_refs = reviewed_member_refs(cluster);
    if decision.recommendation_confidence != cluster.recommendation_confidence
        || decision.recommended_canonical_member_ref != cluster.recommended_canonical.member_ref
        || decision.reviewed_member_refs != member_refs
    {
        return Err("duplicate-canonical-decision-cluster-binding-invalid".into());
    }
    match decision.disposition {
        DuplicateCanonicalDecisionDisposition::Selected => {
            let selected = decision
                .selected_canonical_member_ref
                .as_ref()
                .ok_or_else(|| {
                    "duplicate-canonical-decision-selected-member-invalid".to_string()
                })?;
            if !member_refs.contains(selected)
                || decision.selection_matches_recommendation
                    != Some(selected == &cluster.recommended_canonical.member_ref)
                || !decision.canonical_selection_recorded
            {
                return Err("duplicate-canonical-decision-selection-invalid".into());
            }
        }
        DuplicateCanonicalDecisionDisposition::Held => {
            if decision.selected_canonical_member_ref.is_some()
                || decision.selection_matches_recommendation.is_some()
                || decision.canonical_selection_recorded
            {
                return Err("duplicate-canonical-decision-hold-invalid".into());
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn write_immutable_local_duplicate_canonical_decision(
    dossier: &LocalDuplicateCanonicalReviewDossier,
    directory: &Path,
    decision: &LocalDuplicateCanonicalDecision,
) -> Result<PathBuf, String> {
    use std::io::Write as _;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    validate_local_duplicate_canonical_decision(dossier, decision)?;
    let directory_metadata = std::fs::symlink_metadata(directory)
        .map_err(|_| "duplicate-canonical-decision-directory-unavailable".to_string())?;
    if !directory_metadata.is_dir() || directory_metadata.file_type().is_symlink() {
        return Err("duplicate-canonical-decision-directory-unsafe".into());
    }
    let canonical_directory = std::fs::canonicalize(directory)
        .map_err(|_| "duplicate-canonical-decision-directory-unavailable".to_string())?;
    let canonical_source = std::fs::canonicalize(&dossier.source_root)
        .map_err(|_| "duplicate-canonical-decision-source-root-unavailable".to_string())?;
    if canonical_directory.starts_with(canonical_source) {
        return Err("duplicate-canonical-decision-directory-inside-source".into());
    }
    let path = canonical_directory.join(format!(
        "{}-{:020}-{}.json",
        decision.cluster_ref, decision.reviewed_at_ms, decision.decision_id
    ));
    let encoded = serde_json::to_vec_pretty(decision)
        .map_err(|_| "duplicate-canonical-decision-json-invalid".to_string())?;
    if encoded.len() > MAX_DECISION_BYTES {
        return Err("duplicate-canonical-decision-too-large".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|_| "duplicate-canonical-decision-create-failed".to_string())?;
    let result = (|| -> Result<(), String> {
        file.write_all(&encoded)
            .and_then(|_| file.sync_all())
            .map_err(|_| "duplicate-canonical-decision-write-failed".to_string())?;
        let metadata = file
            .metadata()
            .map_err(|_| "duplicate-canonical-decision-metadata-failed".to_string())?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.permissions().mode() & 0o777 != 0o600
        {
            return Err("duplicate-canonical-decision-mode-invalid".into());
        }
        std::fs::File::open(&canonical_directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "duplicate-canonical-decision-directory-sync-failed".to_string())
    })();
    if let Err(error) = result {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}

#[cfg(not(unix))]
pub fn write_immutable_local_duplicate_canonical_decision(
    _dossier: &LocalDuplicateCanonicalReviewDossier,
    _directory: &Path,
    _decision: &LocalDuplicateCanonicalDecision,
) -> Result<PathBuf, String> {
    Err("duplicate-canonical-decision-secure-mode-unsupported".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::{ArchiveKind, MetadataEvidence};

    fn millis(value: std::io::Result<std::time::SystemTime>) -> u64 {
        filesystem_millis(value)
    }

    fn member(
        root: &Path,
        name: &str,
        member_ref: char,
        metadata_ref: char,
    ) -> LocalDuplicateCanonicalReviewMember {
        let path = root.join(name);
        std::fs::write(&path, b"same").unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        LocalDuplicateCanonicalReviewMember {
            member_ref: member_ref.to_string().repeat(64),
            metadata_fingerprint: metadata_ref.to_string().repeat(64),
            absolute_source_path: path.to_string_lossy().into_owned(),
            relative_path: name.into(),
            source_context: "test".into(),
            archive_kind: ArchiveKind::Document,
            bytes: metadata.len(),
            created_ms: millis(metadata.created()),
            modified_ms: millis(metadata.modified()),
            production_time_ms: 10,
            production_time_source: "embedded:pdf:CreationDate".into(),
            production_time_confidence: "high".into(),
            content_title: Some("title".into()),
            content_authors: vec![],
            content_context: vec![],
            duration_ms: None,
            dataset_profile_present: false,
            review_reasons: vec!["exact-duplicate-content-needs-canonical-selection".into()],
            metadata_evidence: vec![MetadataEvidence {
                field: "production-date".into(),
                value: "2026-01-01".into(),
                source: "embedded:pdf:CreationDate".into(),
                confidence: "high".into(),
            }],
            transfer_blocked_reason: None,
            filesystem_stable_at_export: true,
        }
    }

    fn dossier(root: &Path) -> LocalDuplicateCanonicalReviewDossier {
        let canonical = member(root, "canonical.pdf", 'a', 'c');
        let alternative = member(root, "copy.pdf", 'b', 'd');
        let mut dossier = LocalDuplicateCanonicalReviewDossier {
            schema_version: LOCAL_DUPLICATE_CANONICAL_REVIEW_DOSSIER_SCHEMA_VERSION,
            schema_kind: LOCAL_DUPLICATE_CANONICAL_REVIEW_DOSSIER_SCHEMA_KIND.into(),
            dossier_id: String::new(),
            exported_at_ms: 100,
            observed_at_ms: 90,
            canonical_review_lineage_id: "e".repeat(64),
            duplicate_audit_lineage_id: "f".repeat(64),
            source_root: root.to_string_lossy().into_owned(),
            production_time_precedence: PRODUCTION_TIME_PRECEDENCE.map(str::to_string).to_vec(),
            filename_dates_are_auxiliary: true,
            cluster_count: 1,
            candidate_count: 2,
            candidate_bytes: 8,
            redundant_bytes: 4,
            clusters: vec![LocalDuplicateCanonicalReviewCluster {
                cluster_ref: "1".repeat(64),
                bytes_per_candidate: 4,
                candidate_count: 2,
                redundant_bytes: 4,
                recommendation_confidence: "high".into(),
                recommendation_reason_codes: vec!["embedded-production-time-preferred".into()],
                recommended_canonical: canonical,
                alternatives: vec![alternative],
                requires_human_confirmation: true,
            }],
            local_sensitive_metadata: true,
            naruon_submission_allowed: false,
            automatic_discard_allowed: false,
            human_confirmation_required_for_every_cluster: true,
            mutation_performed: false,
        };
        dossier.dossier_id = local_duplicate_canonical_review_dossier_id(&dossier);
        dossier
    }

    #[test]
    fn verifies_dossier_and_records_selection_without_discard_authority() {
        let source = tempfile::tempdir().unwrap();
        let dossier = dossier(source.path());
        let summary = verify_local_duplicate_canonical_review_dossier(&dossier, 200).unwrap();
        assert_eq!(summary.cluster_count, 1);
        assert_eq!(summary.filesystem_stable_candidate_count, 2);
        assert!(!summary.contains_local_paths);
        assert!(!summary.canonical_decision_created);
        assert!(!summary.discard_authorization);

        let selected = create_local_duplicate_canonical_decision(
            &dossier,
            &"1".repeat(64),
            DuplicateCanonicalDecisionDisposition::Selected,
            Some(&"b".repeat(64)),
            300,
            "human:test",
            "The embedded metadata and content context were inspected.",
        )
        .unwrap();
        assert_eq!(selected.selection_matches_recommendation, Some(false));
        assert!(selected.canonical_selection_recorded);
        assert!(!selected.discard_authorization);
        assert!(!selected.mutation_performed);
        validate_local_duplicate_canonical_decision(&dossier, &selected).unwrap();

        let held = create_local_duplicate_canonical_decision(
            &dossier,
            &"1".repeat(64),
            DuplicateCanonicalDecisionDisposition::Held,
            None,
            301,
            "human:test",
            "More context is required before selecting a canonical copy.",
        )
        .unwrap();
        assert!(!held.canonical_selection_recorded);
        assert!(held.selected_canonical_member_ref.is_none());
        validate_local_duplicate_canonical_decision(&dossier, &held).unwrap();

        assert!(create_local_duplicate_canonical_decision(
            &dossier,
            &"1".repeat(64),
            DuplicateCanonicalDecisionDisposition::Selected,
            Some(&"9".repeat(64)),
            302,
            "human:test",
            "Invalid member must be rejected.",
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn fails_closed_on_tampering_or_source_drift_and_writes_once_as_mode_0600() {
        use std::os::unix::fs::PermissionsExt;

        let source = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let dossier = dossier(source.path());
        let decision = create_local_duplicate_canonical_decision(
            &dossier,
            &"1".repeat(64),
            DuplicateCanonicalDecisionDisposition::Selected,
            Some(&"a".repeat(64)),
            400,
            "human:test",
            "The recommended canonical copy was manually reviewed.",
        )
        .unwrap();
        let path =
            write_immutable_local_duplicate_canonical_decision(&dossier, private.path(), &decision)
                .unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(write_immutable_local_duplicate_canonical_decision(
            &dossier,
            private.path(),
            &decision,
        )
        .is_err());

        let mut tampered = dossier.clone();
        tampered.clusters[0].redundant_bytes += 1;
        assert!(validate_local_duplicate_canonical_review_dossier(&tampered).is_err());

        std::fs::write(source.path().join("copy.pdf"), b"changed").unwrap();
        assert!(verify_local_duplicate_canonical_review_dossier(&dossier, 500).is_err());
        assert!(validate_local_duplicate_canonical_decision(&dossier, &decision).is_err());
    }
}
