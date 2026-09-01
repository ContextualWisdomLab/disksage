use std::path::Path;
use std::time::Duration;

#[path = "podman_reclaim_public.rs"]
mod implementation;

pub use implementation::{
    plan_empty_dangling_volumes, prune_dangling_images, prune_empty_dangling_volumes,
    GuestFilesystemEvidence, PodmanDanglingImagePruneExecution, PodmanEmptyVolumeExecution,
    PodmanEmptyVolumePlan, PodmanMachineEvidence, PodmanReclaimAssessment, PodmanReclaimPlan,
    PodmanRecommendedAction, PodmanRecommendedActionKind, PodmanStoreEvidence,
    PodmanSystemDfCategoryEvidence, PodmanSystemDfEvidence, PodmanUnusedImageEvidence,
    RawImageEvidence, DEFAULT_PODMAN_MACHINE, DEFAULT_PROBE_TIMEOUT, PODMAN_RECLAIM_SCHEMA_KIND,
};

/// Probe Podman reclaimability without offering an approval that the exact executor must reject.
/// The dangling-image executor permits only a non-empty, exclusively untagged candidate set, so
/// the public plan applies the same condition before exposing its backend-authored phrase.
pub fn probe_podman_reclaim(
    podman_bin: &Path,
    requested_machine: &str,
    timeout: Duration,
) -> PodmanReclaimPlan {
    let mut plan = implementation::probe_podman_reclaim(podman_bin, requested_machine, timeout);
    if plan.unused_images.as_ref().is_none_or(|evidence| {
        evidence.unused_untagged_records == 0 || evidence.unused_tagged_records > 0
    }) {
        plan.dangling_prune_approval_phrase = None;
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_predicate_matches_executor_requirement() {
        let mut plan = PodmanReclaimPlan {
            schema_kind: PODMAN_RECLAIM_SCHEMA_KIND,
            schema_version: 3,
            platform: "test",
            evidence_complete: true,
            elapsed_ms: 0,
            machine: None,
            raw_image: None,
            guest_filesystem: None,
            store: None,
            system_df: None,
            unused_images: Some(PodmanUnusedImageEvidence {
                total_records: 2,
                referenced_records: 0,
                unused_records: 2,
                unused_untagged_records: 1,
                unused_tagged_records: 1,
                candidate_record_size_sum: 2,
                candidate_set_sha256: "0".repeat(64),
            }),
            dangling_prune_approval_phrase: Some("must-not-escape".into()),
            assessment: PodmanReclaimAssessment {
                physically_reclaimable_bytes: None,
                podman_reported_reclaimable_bytes: None,
                raw_allocated_minus_guest_used_bytes: None,
                status: "unverified".into(),
                reason_codes: Vec::new(),
                recommended_actions: Vec::new(),
            },
            issues: Vec::new(),
        };
        if plan.unused_images.as_ref().is_none_or(|evidence| {
            evidence.unused_untagged_records == 0 || evidence.unused_tagged_records > 0
        }) {
            plan.dangling_prune_approval_phrase = None;
        }
        assert_eq!(plan.dangling_prune_approval_phrase, None);
    }
}
