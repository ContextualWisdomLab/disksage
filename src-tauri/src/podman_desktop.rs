//! Desktop-safe projection of read-only Podman reclaim evidence.
//!
//! The headless `podman_reclaim` module intentionally gathers more local detail than the
//! desktop needs. This module converts that report into a bounded, privacy-safe contract
//! that contains measurements and stable issue codes, but never machine names, paths,
//! image identifiers, tags, or shell command text.

#![deny(missing_docs)]

use crate::podman_reclaim::{
    probe_podman_reclaim, PodmanReclaimPlan, PodmanRecommendedActionKind, DEFAULT_PODMAN_MACHINE,
    DEFAULT_PROBE_TIMEOUT,
};
use serde::Serialize;
use std::path::Path;

/// Stable schema identifier for the desktop-safe Podman evidence response.
pub const PODMAN_DESKTOP_SCHEMA_KIND: &str = "disksage.podman-desktop-evidence";

/// Capacity observations displayed independently so logical size is never confused with
/// host allocation or verified physical reclaimability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanDesktopCapacityEvidence {
    /// Podman machine disk capacity configured by the operator, when available.
    pub configured_disk_bytes: Option<u64>,
    /// Logical length of the VM raw image file, when available.
    pub raw_logical_bytes: Option<u64>,
    /// Host blocks currently allocated to the VM raw image, when supported by the host.
    pub host_allocated_bytes: Option<u64>,
    /// Total bytes reported by the guest root filesystem.
    pub guest_total_bytes: Option<u64>,
    /// Used bytes reported by the guest root filesystem.
    pub guest_used_bytes: Option<u64>,
    /// Available bytes reported by the guest root filesystem.
    pub guest_available_bytes: Option<u64>,
    /// Bytes Podman reports as allocated to its graph root inside the guest.
    pub graph_root_allocated_bytes: Option<u64>,
    /// Bytes Podman reports as used in its graph root inside the guest.
    pub graph_root_used_bytes: Option<u64>,
}

/// Logical cleanup candidates reported by Podman without exposing local identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanDesktopCandidateEvidence {
    /// Logical image candidate bytes reported by `podman system df`.
    pub image_candidate_bytes: Option<u64>,
    /// Logical stopped-container candidate bytes reported by `podman system df`.
    pub stopped_container_candidate_bytes: Option<u64>,
    /// Logical local-volume candidate bytes reported by `podman system df`.
    pub volume_candidate_bytes: Option<u64>,
    /// Count of exact image records with no container references.
    pub unused_image_records: Option<u64>,
    /// Count of stopped containers observed in the Podman store.
    pub stopped_container_records: Option<u64>,
    /// SHA-256 commitment to exact unused image identifiers, tags, and sizes.
    pub image_candidate_set_sha256: Option<String>,
}

/// Separate review boundaries for image, stopped-container, and volume decisions.
///
/// These booleans are advisory only. They do not authorize or execute any mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanDesktopReviewBoundaries {
    /// Whether image candidates require an independent human review decision.
    pub image_review_required: bool,
    /// Whether stopped-container candidates require an independent human review decision.
    pub stopped_container_review_required: bool,
    /// Whether volume candidates require an independent human review decision.
    pub volume_review_required: bool,
}

/// Privacy-safe, read-only Podman evidence returned to the desktop frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanDesktopEvidence {
    /// Stable schema identifier used by frontend validation.
    pub schema_kind: &'static str,
    /// Schema version for compatibility checks.
    pub schema_version: u32,
    /// Operating-system family that produced the evidence.
    pub platform: &'static str,
    /// True only when the probe is complete and no projected issue invalidates the evidence.
    pub evidence_complete: bool,
    /// Bounded probe duration in milliseconds.
    pub elapsed_ms: u64,
    /// Capacity observations kept in distinct semantic categories.
    pub capacity: PodmanDesktopCapacityEvidence,
    /// Logical candidate observations kept separate by Podman object class.
    pub candidates: PodmanDesktopCandidateEvidence,
    /// Separate human-review boundaries for images, stopped containers, and volumes.
    pub review_boundaries: PodmanDesktopReviewBoundaries,
    /// Verified host physical reclaimability; intentionally `None` until before/after proof exists.
    pub physically_reclaimable_bytes: Option<u64>,
    /// Sum of Podman-reported logical candidate bytes, not physical reclaim proof.
    pub podman_reported_reclaimable_bytes: Option<u64>,
    /// Observed host-allocation minus guest-used gap, not physical reclaim proof.
    pub raw_allocated_minus_guest_used_bytes: Option<u64>,
    /// Stable assessment status such as `unverified`.
    pub assessment_status: String,
    /// Stable, non-sensitive assessment reason codes.
    pub reason_codes: Vec<String>,
    /// Stable, non-sensitive probe issue codes with dynamic details removed.
    pub issue_codes: Vec<String>,
    /// User-facing safety statements that define the evidence boundary.
    pub notices: Vec<String>,
}

/// Return true only for a canonical lowercase hexadecimal SHA-256 encoding.
fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// Return the bounded kebab-case prefix of an untrusted diagnostic code when it is safe.
fn stable_code_prefix(value: &str) -> Option<String> {
    let code = value.split(':').next().unwrap_or_default();
    let valid = !code.is_empty()
        && code.len() <= 96
        && code
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    valid.then(|| code.to_string())
}

/// Reduce untrusted local diagnostic text to a bounded kebab-case issue code.
///
/// The prefix before the first colon is accepted only when it starts with a lowercase ASCII
/// letter, contains lowercase ASCII letters, digits, or hyphens, and is at most 96 bytes. Paths,
/// socket names, whitespace, uppercase text, Unicode, underscores, and empty prefixes fall back to
/// one stable generic code rather than crossing the desktop IPC boundary.
fn stable_issue_code(value: &str) -> String {
    stable_code_prefix(value).unwrap_or_else(|| "podman-evidence-error".to_string())
}

/// Return whether a matching recommended action requires independent human approval.
fn has_action(plan: &PodmanReclaimPlan, kind: PodmanRecommendedActionKind) -> bool {
    plan.assessment
        .recommended_actions
        .iter()
        .any(|action| action.kind == kind && action.requires_human_approval)
}

/// Convert a detailed headless Podman plan into the desktop-safe contract.
///
/// The conversion removes machine names, all local paths, graph-root locations, image IDs,
/// tags, command output, and dynamic error details. Invalid candidate fingerprints, assessment
/// codes, unverified physical-reclaim claims, or any projected issue fail closed by clearing
/// unsafe data and marking the response incomplete. Positive candidates conservatively force the
/// corresponding review boundary even if an upstream recommended-action record is missing.
pub fn redact_podman_reclaim_plan(plan: PodmanReclaimPlan) -> PodmanDesktopEvidence {
    let mut issue_codes = plan
        .issues
        .iter()
        .map(|issue| stable_issue_code(issue))
        .collect::<Vec<_>>();

    let candidate_fingerprint = plan
        .unused_images
        .as_ref()
        .map(|images| images.candidate_set_sha256.clone());
    let fingerprint_valid = candidate_fingerprint.as_deref().is_none_or(valid_sha256);
    if !fingerprint_valid {
        issue_codes.push("podman-desktop-invalid-candidate-fingerprint".to_string());
    }

    let assessment_status_valid = plan.assessment.status == "unverified";
    let assessment_status = if assessment_status_valid {
        plan.assessment.status.clone()
    } else {
        "unverified".to_string()
    };
    let mut assessment_codes_valid = assessment_status_valid;
    let mut reason_codes = plan
        .assessment
        .reason_codes
        .iter()
        .map(|reason| {
            stable_code_prefix(reason).unwrap_or_else(|| {
                assessment_codes_valid = false;
                "podman-assessment-error".to_string()
            })
        })
        .collect::<Vec<_>>();
    reason_codes.sort();
    reason_codes.dedup();
    if !assessment_codes_valid {
        issue_codes.push("podman-desktop-invalid-assessment-code".to_string());
    }

    let physical_reclaim_claim_valid = plan.assessment.physically_reclaimable_bytes.is_none();
    let physically_reclaimable_bytes = if physical_reclaim_claim_valid {
        plan.assessment.physically_reclaimable_bytes
    } else {
        issue_codes.push("podman-desktop-unverified-physical-reclaim-claim".to_string());
        None
    };

    issue_codes.sort();
    issue_codes.dedup();
    let issues_absent = issue_codes.is_empty();

    let capacity = PodmanDesktopCapacityEvidence {
        configured_disk_bytes: plan
            .machine
            .as_ref()
            .and_then(|machine| machine.configured_disk_bytes),
        raw_logical_bytes: plan.raw_image.as_ref().map(|image| image.logical_bytes),
        host_allocated_bytes: plan
            .raw_image
            .as_ref()
            .and_then(|image| image.allocated_bytes),
        guest_total_bytes: plan
            .guest_filesystem
            .as_ref()
            .map(|guest| guest.total_bytes),
        guest_used_bytes: plan.guest_filesystem.as_ref().map(|guest| guest.used_bytes),
        guest_available_bytes: plan
            .guest_filesystem
            .as_ref()
            .map(|guest| guest.available_bytes),
        graph_root_allocated_bytes: plan
            .store
            .as_ref()
            .map(|store| store.graph_root_allocated_bytes),
        graph_root_used_bytes: plan.store.as_ref().map(|store| store.graph_root_used_bytes),
    };

    let candidates = PodmanDesktopCandidateEvidence {
        image_candidate_bytes: plan
            .system_df
            .as_ref()
            .map(|evidence| evidence.images.reclaimable_bytes),
        stopped_container_candidate_bytes: plan
            .system_df
            .as_ref()
            .map(|evidence| evidence.containers.reclaimable_bytes),
        volume_candidate_bytes: plan
            .system_df
            .as_ref()
            .map(|evidence| evidence.local_volumes.reclaimable_bytes),
        unused_image_records: plan
            .unused_images
            .as_ref()
            .map(|images| images.unused_records),
        stopped_container_records: plan.store.as_ref().map(|store| store.containers_stopped),
        image_candidate_set_sha256: candidate_fingerprint.filter(|_| fingerprint_valid),
    };

    let image_review_required = has_action(&plan, PodmanRecommendedActionKind::ReviewUnusedImages)
        || candidates
            .image_candidate_bytes
            .is_some_and(|bytes| bytes > 0)
        || candidates
            .unused_image_records
            .is_some_and(|records| records > 0);
    let stopped_container_review_required =
        has_action(&plan, PodmanRecommendedActionKind::ReviewStoppedContainers)
            || candidates
                .stopped_container_candidate_bytes
                .is_some_and(|bytes| bytes > 0)
            || candidates
                .stopped_container_records
                .is_some_and(|records| records > 0);
    let volume_review_required =
        has_action(&plan, PodmanRecommendedActionKind::ReviewUnusedVolumes)
            || candidates
                .volume_candidate_bytes
                .is_some_and(|bytes| bytes > 0);

    PodmanDesktopEvidence {
        schema_kind: PODMAN_DESKTOP_SCHEMA_KIND,
        schema_version: 1,
        platform: plan.platform,
        evidence_complete: plan.evidence_complete
            && fingerprint_valid
            && assessment_codes_valid
            && physical_reclaim_claim_valid
            && issues_absent,
        elapsed_ms: plan.elapsed_ms,
        capacity,
        candidates,
        review_boundaries: PodmanDesktopReviewBoundaries {
            image_review_required,
            stopped_container_review_required,
            volume_review_required,
        },
        physically_reclaimable_bytes,
        podman_reported_reclaimable_bytes: plan.assessment.podman_reported_reclaimable_bytes,
        raw_allocated_minus_guest_used_bytes: plan
            .assessment
            .raw_allocated_minus_guest_used_bytes,
        assessment_status,
        reason_codes,
        issue_codes,
        notices: vec![
            "Podman-reported logical candidates are not verified host physical reclaimability."
                .to_string(),
            "This desktop surface exposes no prune, remove, machine lifecycle, TRIM, or raw-image mutation command."
                .to_string(),
        ],
    }
}

/// Run the bounded read-only Podman probe and return only the desktop-safe projection.
///
/// The command passes an argument vector directly to `std::process::Command` through the
/// headless probe. It never constructs a shell command and never executes a mutation.
pub fn inspect_podman_reclaim() -> PodmanDesktopEvidence {
    redact_podman_reclaim_plan(probe_podman_reclaim(
        Path::new("podman"),
        DEFAULT_PODMAN_MACHINE,
        DEFAULT_PROBE_TIMEOUT,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::podman_reclaim::{
        GuestFilesystemEvidence, PodmanHostCompactionPlan, PodmanMachineEvidence, PodmanReclaimAssessment,
        PodmanRecommendedAction, PodmanStoreEvidence, PodmanSystemDfCategoryEvidence,
        PodmanSystemDfEvidence, PodmanUnusedImageEvidence, RawImageEvidence,
        PODMAN_RECLAIM_SCHEMA_KIND,
    };

    /// Build a deterministic Podman `system df` category fixture with one active record.
    fn category(reclaimable_bytes: u64) -> PodmanSystemDfCategoryEvidence {
        PodmanSystemDfCategoryEvidence {
            total: 2,
            active: 1,
            size_bytes: reclaimable_bytes.saturating_add(10),
            reclaimable_bytes,
        }
    }

    /// Build a complete privacy-sensitive headless plan used by redaction regression tests.
    fn complete_plan() -> PodmanReclaimPlan {
        PodmanReclaimPlan {
            schema_kind: PODMAN_RECLAIM_SCHEMA_KIND,
            schema_version: 3,
            platform: "macos",
            evidence_complete: true,
            elapsed_ms: 17,
            machine: Some(PodmanMachineEvidence {
                name: "private-machine".to_string(),
                state: "running".to_string(),
                configured_disk_bytes: Some(1000),
            }),
            raw_image: Some(RawImageEvidence {
                path: "/Users/private/.local/share/private-machine.raw".to_string(),
                logical_bytes: 900,
                allocated_bytes: Some(700),
                identity_sha256: Some("d".repeat(64)),
                freshness_sha256: Some("e".repeat(64)),
            }),
            guest_filesystem: Some(GuestFilesystemEvidence {
                total_bytes: 800,
                used_bytes: 500,
                available_bytes: 300,
            }),
            store: Some(PodmanStoreEvidence {
                graph_root: "/var/home/private/containers".to_string(),
                graph_root_allocated_bytes: 600,
                graph_root_used_bytes: 450,
                images: 4,
                containers_total: 3,
                containers_running: 1,
                containers_stopped: 2,
            }),
            system_df: Some(PodmanSystemDfEvidence {
                images: category(200),
                containers: category(30),
                local_volumes: category(70),
            }),
            unused_images: Some(PodmanUnusedImageEvidence {
                total_records: 4,
                referenced_records: 2,
                unused_records: 2,
                unused_untagged_records: 1,
                unused_tagged_records: 1,
                candidate_record_size_sum: 200,
                candidate_set_sha256: "a".repeat(64),
            }),
            dangling_prune_approval_phrase: None,
            host_compaction: PodmanHostCompactionPlan {
                supported: false,
                machine_identity_sha256: Some("f".repeat(64)),
                backing_file_identity_sha256: Some("d".repeat(64)),
                backing_file_freshness_sha256: Some("e".repeat(64)),
                active_container_count: Some(1),
                stop_command: None,
                compaction_command: None,
                exact_approval_phrase: None,
                rollback_policy: "require-runtime-native-rollback-before-execution",
                restart_policy: "restore-observed-running-state-after-success-or-rollback",
                blockers: vec!["active-container-count-must-be-zero".to_string()],
                execution_performed: false,
            },
            assessment: PodmanReclaimAssessment {
                physically_reclaimable_bytes: None,
                podman_reported_reclaimable_bytes: Some(300),
                raw_allocated_minus_guest_used_bytes: Some(200),
                status: "unverified".to_string(),
                reason_codes: vec!["host-physical-reclaim-unverified".to_string()],
                recommended_actions: vec![
                    PodmanRecommendedAction {
                        kind: PodmanRecommendedActionKind::ReviewUnusedImages,
                        requires_human_approval: true,
                        rationale: "image review".to_string(),
                    },
                    PodmanRecommendedAction {
                        kind: PodmanRecommendedActionKind::ReviewStoppedContainers,
                        requires_human_approval: true,
                        rationale: "container review".to_string(),
                    },
                    PodmanRecommendedAction {
                        kind: PodmanRecommendedActionKind::ReviewUnusedVolumes,
                        requires_human_approval: true,
                        rationale: "volume review".to_string(),
                    },
                ],
            },
            issues: vec![],
        }
    }

    /// Verify that the desktop contract keeps capacity categories separate and redacts local data.
    #[test]
    fn projection_keeps_measurements_separate_and_removes_private_context() {
        let evidence = redact_podman_reclaim_plan(complete_plan());
        assert!(evidence.evidence_complete);
        assert_eq!(evidence.capacity.configured_disk_bytes, Some(1000));
        assert_eq!(evidence.capacity.raw_logical_bytes, Some(900));
        assert_eq!(evidence.capacity.host_allocated_bytes, Some(700));
        assert_eq!(evidence.capacity.guest_used_bytes, Some(500));
        assert_eq!(evidence.candidates.image_candidate_bytes, Some(200));
        assert_eq!(
            evidence.candidates.stopped_container_candidate_bytes,
            Some(30)
        );
        assert_eq!(evidence.candidates.volume_candidate_bytes, Some(70));
        assert_eq!(
            evidence.candidates.image_candidate_set_sha256,
            Some("a".repeat(64))
        );
        let json = serde_json::to_string(&evidence).unwrap();
        assert!(!json.contains("private-machine"));
        assert!(!json.contains("/Users/private"));
        assert!(!json.contains("/var/home/private"));
    }

    /// Verify that image, stopped-container, and volume review decisions never authorize each other.
    #[test]
    fn image_container_and_volume_reviews_remain_separate() {
        let evidence = redact_podman_reclaim_plan(complete_plan());
        assert!(evidence.review_boundaries.image_review_required);
        assert!(evidence.review_boundaries.stopped_container_review_required);
        assert!(evidence.review_boundaries.volume_review_required);

        let mut plan = complete_plan();
        plan.store = None;
        plan.system_df = None;
        plan.unused_images = None;
        plan.evidence_complete = false;
        plan.assessment.recommended_actions = vec![PodmanRecommendedAction {
            kind: PodmanRecommendedActionKind::InvestigateApi,
            requires_human_approval: false,
            rationale: "diagnostic only".to_string(),
        }];
        let evidence = redact_podman_reclaim_plan(plan);
        assert!(!evidence.review_boundaries.image_review_required);
        assert!(!evidence.review_boundaries.stopped_container_review_required);
        assert!(!evidence.review_boundaries.volume_review_required);
    }

    /// Verify that dynamic local diagnostic details are removed and duplicate stable codes collapse.
    #[test]
    fn dynamic_issue_details_are_redacted_and_deduplicated() {
        let mut plan = complete_plan();
        plan.evidence_complete = false;
        plan.issues = vec![
            "podman-info-failed:/Users/alice/private.sock".to_string(),
            "podman-info-failed:duplicate detail".to_string(),
            "podman-images-timeout".to_string(),
        ];
        let evidence = redact_podman_reclaim_plan(plan);
        assert!(!evidence.evidence_complete);
        assert_eq!(
            evidence.issue_codes,
            vec![
                "podman-images-timeout".to_string(),
                "podman-info-failed".to_string(),
            ]
        );
        assert!(!serde_json::to_string(&evidence)
            .unwrap()
            .contains("Users/alice"));
    }

    /// Verify that malformed candidate fingerprints fail closed without discarding safe measurements.
    #[test]
    fn invalid_fingerprint_fails_closed_without_hiding_other_evidence() {
        let mut plan = complete_plan();
        plan.unused_images.as_mut().unwrap().candidate_set_sha256 = "BAD".to_string();
        let evidence = redact_podman_reclaim_plan(plan);
        assert!(!evidence.evidence_complete);
        assert_eq!(evidence.candidates.image_candidate_set_sha256, None);
        assert!(evidence
            .issue_codes
            .contains(&"podman-desktop-invalid-candidate-fingerprint".to_string()));
        assert_eq!(evidence.candidates.image_candidate_bytes, Some(200));
    }

    /// Verify that missing optional observations remain unknown rather than becoming false zeroes.
    #[test]
    fn absent_optional_evidence_stays_unknown_instead_of_becoming_zero() {
        let mut plan = complete_plan();
        plan.machine = None;
        plan.raw_image = None;
        plan.guest_filesystem = None;
        plan.store = None;
        plan.system_df = None;
        plan.unused_images = None;
        plan.evidence_complete = false;
        let evidence = redact_podman_reclaim_plan(plan);
        assert_eq!(evidence.capacity.configured_disk_bytes, None);
        assert_eq!(evidence.capacity.raw_logical_bytes, None);
        assert_eq!(evidence.capacity.host_allocated_bytes, None);
        assert_eq!(evidence.capacity.guest_total_bytes, None);
        assert_eq!(evidence.capacity.guest_used_bytes, None);
        assert_eq!(evidence.capacity.guest_available_bytes, None);
        assert_eq!(evidence.capacity.graph_root_allocated_bytes, None);
        assert_eq!(evidence.capacity.graph_root_used_bytes, None);
        assert_eq!(evidence.candidates.image_candidate_bytes, None);
        assert_eq!(evidence.candidates.stopped_container_candidate_bytes, None);
        assert_eq!(evidence.candidates.volume_candidate_bytes, None);
        assert_eq!(evidence.candidates.unused_image_records, None);
        assert_eq!(evidence.candidates.stopped_container_records, None);
        assert_eq!(evidence.candidates.image_candidate_set_sha256, None);
    }

    /// Verify stable fallback issue codes and canonical lowercase SHA-256 validation.
    #[test]
    fn issue_code_fallback_and_fingerprint_validation_are_stable() {
        assert_eq!(stable_issue_code(""), "podman-evidence-error");
        assert_eq!(stable_issue_code(":private"), "podman-evidence-error");
        assert_eq!(
            stable_issue_code("/Users/alice/private-machine.sock"),
            "podman-evidence-error"
        );
        assert_eq!(stable_issue_code("UPPERCASE"), "podman-evidence-error");
        assert_eq!(stable_issue_code("unsafe_code"), "podman-evidence-error");
        assert_eq!(stable_issue_code("stable:private"), "stable");
        assert!(valid_sha256(&"0".repeat(64)));
        assert!(!valid_sha256(&"A".repeat(64)));
        assert!(!valid_sha256("short"));
    }
}
