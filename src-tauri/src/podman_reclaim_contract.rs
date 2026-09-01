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

fn dangling_image_approval_is_executable(evidence: &PodmanUnusedImageEvidence) -> bool {
    evidence.unused_untagged_records > 0
        && evidence.unused_untagged_records
            <= u64::try_from(implementation::MAX_EXACT_DELETE_IDS).unwrap_or(u64::MAX)
}

/// Probe Podman reclaimability without offering an approval that the exact executor must reject.
/// Tagged unused images may coexist with executable dangling candidates because the executor
/// removes only immutable untagged IDs while binding approval to the complete candidate snapshot.
pub fn probe_podman_reclaim(
    podman_bin: &Path,
    requested_machine: &str,
    timeout: Duration,
) -> PodmanReclaimPlan {
    let mut plan = implementation::probe_podman_reclaim(podman_bin, requested_machine, timeout);
    if plan
        .unused_images
        .as_ref()
        .is_none_or(|evidence| !dangling_image_approval_is_executable(evidence))
    {
        plan.dangling_prune_approval_phrase = None;
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn probe_with_images(images_json: &str) -> PodmanReclaimPlan {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "disksage-podman-contract-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("fixture root must be creatable");
        let script = root.join("podman-fixture");
        let escaped_root = root.to_string_lossy().replace('"', "\\\"");
        let body = format!(
            r#"#!/bin/sh
if [ "$1" = "machine" ] && [ "$2" = "inspect" ]; then
  printf '%s\n' '[{{"ConfigDir":{{"Path":"{escaped_root}"}},"Name":"contract-machine","State":"running","Resources":{{"DiskSize":1}}}}]'
  exit 0
fi
if [ "$1" = "--connection" ] && [ "$3" = "images" ]; then
  cat <<'DISKSAGE_IMAGES'
{images_json}
DISKSAGE_IMAGES
  exit 0
fi
exit 1
"#
        );
        fs::write(&script, body).expect("fixture script must be writable");
        let mut permissions = fs::metadata(&script)
            .expect("fixture script metadata must be readable")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&script, permissions).expect("fixture script must be executable");

        let plan = probe_podman_reclaim(&script, "contract-machine", Duration::from_secs(2));
        let _ = fs::remove_dir_all(root);
        plan
    }

    #[cfg(unix)]
    #[test]
    fn mixed_tagged_and_untagged_images_preserve_executable_approval() {
        let untagged = "a".repeat(64);
        let tagged = "b".repeat(64);
        let images = format!(
            r#"[{{"Id":"{untagged}","RepoTags":[],"Containers":0,"Size":100}},{{"Id":"{tagged}","RepoTags":["keep:latest"],"Containers":0,"Size":200}}]"#
        );

        let plan = probe_with_images(&images);
        let evidence = plan.unused_images.expect("image evidence must be collected");
        assert_eq!(evidence.unused_untagged_records, 1);
        assert_eq!(evidence.unused_tagged_records, 1);
        assert!(plan.dangling_prune_approval_phrase.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn approval_respects_exact_delete_id_boundary() {
        let images_json = |count: usize| {
            let records = (1..=count)
                .map(|index| {
                    format!(
                        r#"{{"Id":"{index:064x}","RepoTags":[],"Containers":0,"Size":1}}"#
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("[{records}]")
        };
        let limit = implementation::MAX_EXACT_DELETE_IDS;

        let at_limit = probe_with_images(&images_json(limit));
        assert!(at_limit.dangling_prune_approval_phrase.is_some());

        let above_limit = probe_with_images(&images_json(limit + 1));
        assert!(above_limit.dangling_prune_approval_phrase.is_none());
    }
}
