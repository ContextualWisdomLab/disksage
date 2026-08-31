#![cfg(unix)]

use disksage_lib::container_orphan_reclaim::{
    execute_container_orphan_prune, probe_container_orphans_with_receipt_dir,
    ContainerRuntimeKind, ContainerRuntimeTarget, OrphanCategory,
};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

#[test]
fn build_cache_prune_targets_only_the_freshly_reviewed_record_id() {
    let temp = tempfile::tempdir().unwrap();
    let receipt_dir = temp.path().join("receipts");
    std::fs::create_dir(&receipt_dir).unwrap();
    std::fs::set_permissions(&receipt_dir, std::fs::Permissions::from_mode(0o700)).unwrap();

    let log_path = temp.path().join("docker.log");
    let docker_path = temp.path().join("docker");
    let script = format!(
        r#"#!/bin/sh
printf '%s\n' "$*" >> '{}'
case "$*" in
  *" info") exit 0 ;;
  *"container ps"*) exit 0 ;;
  *" images "*) exit 0 ;;
  *"volume ls"*) exit 0 ;;
  *"network ls"*) exit 0 ;;
  *"buildx du --format json"*)
    printf '%s\n' '{{"ID":"cache123","Reclaimable":true}}'
    exit 0
    ;;
  *"buildx prune --all --filter id~=^(cache123)$ --force") exit 0 ;;
  *)
    printf '%s\n' "unexpected command: $*" >&2
    exit 23
    ;;
esac
"#,
        log_path.display()
    );
    std::fs::write(&docker_path, script).unwrap();
    std::fs::set_permissions(&docker_path, std::fs::Permissions::from_mode(0o700)).unwrap();

    let target = ContainerRuntimeTarget::new(
        ContainerRuntimeKind::DockerNative,
        PathBuf::from(&docker_path),
        None,
    )
    .unwrap();

    let plan = probe_container_orphans_with_receipt_dir(&target, &receipt_dir);
    assert!(plan.evidence_complete, "plan issues: {:?}", plan.issues);
    let build_cache = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::BuildCache)
        .expect("BuildKit cache category must be audited for Docker");
    assert_eq!(
        build_cache
            .evidence
            .as_ref()
            .expect("BuildKit evidence")
            .candidate_records,
        1
    );
    let phrase = build_cache
        .approval_phrase
        .as_deref()
        .expect("fresh exact candidate set must be approvable");

    let execution = execute_container_orphan_prune(
        &target,
        OrphanCategory::BuildCache,
        phrase,
        "Reviewed the exact BuildKit cache record.",
        1_800_000_000_000,
        &receipt_dir,
    )
    .expect("exact BuildKit record deletion should be supported");
    assert_eq!(execution.status_code, 0);
    assert!(execution.executed);

    let log = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        log.lines().any(|line| {
            line.ends_with("buildx prune --all --filter id~=^(cache123)$ --force")
        }),
        "exact prune command missing from log: {log}"
    );
    assert!(
        !log.lines().any(|line| {
            line.contains("buildx prune --all --force")
                || (line.contains("buildx prune --all") && !line.contains("--filter id~="))
        }),
        "unfiltered category-wide prune must never run: {log}"
    );
}
