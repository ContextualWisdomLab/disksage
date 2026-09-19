use disksage_lib::dev_artifacts::{find_artifacts, partition_artifacts_by_protection};
use disksage_lib::reclaim_protection::{ProtectionContext, REASON_ORCHESTRATION_LEAD};
use std::fs;
use std::path::{Path, PathBuf};

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

#[test]
fn orchestration_lead_target_is_protected_while_idle_target_remains_reclaimable() {
    let temp = tempfile::tempdir().expect("create fixture root");
    let lead_target = cargo_target(temp.path(), "orchestration-lead-demo");
    let idle_target = cargo_target(temp.path(), "idle-crate");

    let artifacts = find_artifacts(temp.path(), 0, u64::MAX);
    assert!(
        artifacts.iter().any(|artifact| Path::new(&artifact.path) == lead_target),
        "the real generated lead target must first be admitted by ordinary rebuild authority"
    );
    assert!(
        artifacts.iter().any(|artifact| Path::new(&artifact.path) == idle_target),
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
}