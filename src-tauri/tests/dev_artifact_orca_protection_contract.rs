use disksage_lib::dev_artifacts::{
    clean_artifacts, find_artifacts, partition_artifacts_by_protection,
};
use disksage_lib::reclaim_protection::{
    assess_worktree_protections, ProtectionContext, REASON_ORCHESTRATION_LEAD,
    REASON_RECENT_WRITES,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn cargo_target(root: &Path, project_name: &str) -> PathBuf {
    let project = root.join(project_name);
    let target = project.join("target");
    fs::create_dir_all(&target).expect("create Cargo target fixture");
    fs::write(
        project.join("Cargo.toml"),
        format!("[package]\nname = \"{project_name}\"\nversion = \"0.1.0\"\n"),
    )
    .expect("write Cargo manifest");
    fs::write(project.join("Cargo.lock"), b"version = 4\n").expect("write Cargo lockfile");
    fs::write(target.join("generated.bin"), [0x5a; 4096]).expect("write generated artifact");
    target
}

#[cfg(windows)]
fn set_modified_time(path: &Path, modified: SystemTime) {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .expect("open filesystem object for timestamp control")
        .set_times(fs::FileTimes::new().set_modified(modified))
        .expect("set filesystem object modification time");
}

#[cfg(not(windows))]
fn set_modified_time(path: &Path, modified: SystemTime) {
    fs::OpenOptions::new()
        .read(true)
        .open(path)
        .expect("open filesystem object for timestamp control")
        .set_times(fs::FileTimes::new().set_modified(modified))
        .expect("set filesystem object modification time");
}

#[test]
fn orchestration_lead_target_is_protected_while_idle_target_remains_reclaimable() {
    let temp = tempfile::tempdir().expect("create fixture root");
    let lead_target = cargo_target(temp.path(), "orchestration-lead-demo");
    let idle_target = cargo_target(temp.path(), "idle-crate");

    let artifacts = find_artifacts(temp.path(), 0, u64::MAX);
    assert!(
        artifacts
            .iter()
            .any(|artifact| Path::new(&artifact.path) == lead_target),
        "the real generated lead target must first be admitted by ordinary rebuild authority"
    );
    assert!(
        artifacts
            .iter()
            .any(|artifact| Path::new(&artifact.path) == idle_target),
        "the idle control target must first be admitted by ordinary rebuild authority"
    );

    let (reclaimable, protected) =
        partition_artifacts_by_protection(&artifacts, &ProtectionContext::default());

    assert!(
        protected.iter().any(|(artifact, assessment)| {
            Path::new(&artifact.path) == lead_target
                && assessment
                    .reason_codes
                    .iter()
                    .any(|reason| reason == REASON_ORCHESTRATION_LEAD)
        }),
        "an orchestration-lead build root must be withheld with the stable owner reason code"
    );
    assert!(
        reclaimable
            .iter()
            .any(|artifact| Path::new(&artifact.path) == idle_target),
        "an unrelated rebuildable target must remain reclaimable when no protection applies"
    );

    let lead_artifact = artifacts
        .iter()
        .find(|artifact| Path::new(&artifact.path) == lead_target)
        .expect("lead target inventory")
        .clone();
    let journal = temp.path().join("journal.jsonl");
    let results = clean_artifacts(&[lead_artifact], temp.path(), 0, &journal, 1);

    assert_eq!(results.len(), 1);
    assert!(!results[0].ok, "protection must be deletion authority, not display metadata");
    assert!(
        results[0].error.contains(REASON_ORCHESTRATION_LEAD),
        "cleanup rejection must retain the stable protection reason code: {}",
        results[0].error
    );
    assert!(lead_target.exists(), "protected generated root must remain on disk");
    assert!(
        !journal.exists(),
        "protection rejection must occur before mutation journaling"
    );
}

#[test]
fn existing_descendant_recent_write_is_protected_when_root_directory_is_old() {
    let temp = tempfile::tempdir().expect("create fixture root");
    let worktree = temp.path().join("idle-worktree");
    let existing = worktree.join("existing.txt");
    fs::create_dir(&worktree).expect("create candidate worktree");
    fs::write(&existing, b"old\n").expect("create existing descendant");

    let now = SystemTime::now();
    let old = now
        .checked_sub(Duration::from_secs(7_200))
        .expect("old timestamp");
    set_modified_time(&existing, old);
    set_modified_time(&worktree, old);

    fs::write(&existing, b"recent edit\n").expect("modify the existing descendant");
    set_modified_time(&existing, now);
    set_modified_time(&worktree, old);

    let now_unix_secs = now
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_secs();
    let context = ProtectionContext {
        recent_write_window_secs: Some(3_600),
        now_unix_secs: Some(now_unix_secs),
        ..ProtectionContext::default()
    };
    let assessment = assess_worktree_protections(
        &worktree,
        None,
        None,
        &context,
        false,
        false,
        Some((false, false)),
        false,
        Some(false),
        false,
    );

    assert!(
        assessment
            .reason_codes
            .iter()
            .any(|reason| reason == REASON_RECENT_WRITES),
        "an existing descendant edited inside the explicit protection window must veto reclaim even when the ancestor directory mtime is old: {:?}",
        assessment.reason_codes
    );
}
