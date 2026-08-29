use disksage_lib::git_worktree::GitWorktreeActiveUseEvidence;
use disksage_lib::parallels_disk_reclaim::{plan_with_runner, ParallelsCommandRunner};
use std::path::Path;

struct FakeRunner {
    home: String,
}

impl ParallelsCommandRunner for FakeRunner {
    fn run(&self, _: &Path, args: &[&str], _: &str) -> Result<String, String> {
        match args.first().copied() {
            Some("list") => Ok(serde_json::json!([{
                "ID": "vm-123",
                "Name": "Work Windows",
                "Status": "stopped",
                "Home": self.home,
            }])
            .to_string()),
            Some("compact") => Ok(
                "Block size: 8\nTotal blocks: 30000\nAllocated blocks: 15000\nUsed blocks: 2712\n"
                    .into(),
            ),
            _ => Err("unexpected-command".into()),
        }
    }
}

fn inactive() -> GitWorktreeActiveUseEvidence {
    GitWorktreeActiveUseEvidence {
        method: "lsof-recursive-pid".into(),
        assessed: true,
        evidence_complete: true,
        active: false,
        observed_pids: Vec::new(),
        results_truncated: false,
        error: None,
    }
}

#[test]
fn stopped_vm_fake_cli_reports_exact_48_mib_without_authorizing_execution() {
    let temp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    let prlctl = root.join("prlctl");
    let disk_tool = root.join("prl_disk_tool");
    std::fs::write(&prlctl, b"fake").unwrap();
    std::fs::write(&disk_tool, b"fake").unwrap();
    let bundle = root.join("Work Windows.pvm");
    let disk = bundle.join("Work Windows-0.hdd");
    std::fs::create_dir_all(&disk).unwrap();
    std::fs::write(disk.join("DiskDescriptor.xml"), b"descriptor").unwrap();
    let runner = FakeRunner {
        home: bundle.to_string_lossy().into_owned(),
    };

    let plan = plan_with_runner(
        &runner,
        &prlctl,
        &disk_tool,
        "vm-123",
        &bundle,
        &disk,
        123,
        inactive(),
    )
    .unwrap();

    assert_eq!(plan.reclaimable_bytes, Some(48 * 1024 * 1024));
    assert_eq!(plan.vm_status, "stopped");
    assert!(!plan.execution_available);
    assert!(plan
        .blockers
        .contains(&"parallels-compact-execution-not-implemented".into()));
    assert!(plan.next_action.contains("VM을 그대로 유지"));
}

#[test]
fn stopped_vm_id_cannot_authorize_a_different_bundle() {
    let temp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    let prlctl = root.join("prlctl");
    let disk_tool = root.join("prl_disk_tool");
    std::fs::write(&prlctl, b"fake").unwrap();
    std::fs::write(&disk_tool, b"fake").unwrap();
    let requested_bundle = root.join("Requested.pvm");
    let requested_disk = requested_bundle.join("Requested-0.hdd");
    std::fs::create_dir_all(&requested_disk).unwrap();
    std::fs::write(requested_disk.join("DiskDescriptor.xml"), b"descriptor").unwrap();
    let registered_bundle = root.join("Registered.pvm");
    std::fs::create_dir_all(&registered_bundle).unwrap();
    let runner = FakeRunner {
        home: registered_bundle.to_string_lossy().into_owned(),
    };

    let error = plan_with_runner(
        &runner,
        &prlctl,
        &disk_tool,
        "vm-123",
        &requested_bundle,
        &requested_disk,
        123,
        inactive(),
    )
    .unwrap_err();

    assert_eq!(error, "parallels-vm-bundle-mismatch");
}

#[test]
fn plan_fingerprint_changes_when_active_use_evidence_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    let prlctl = root.join("prlctl");
    let disk_tool = root.join("prl_disk_tool");
    std::fs::write(&prlctl, b"fake").unwrap();
    std::fs::write(&disk_tool, b"fake").unwrap();
    let bundle = root.join("Work Windows.pvm");
    let disk = bundle.join("Work Windows-0.hdd");
    std::fs::create_dir_all(&disk).unwrap();
    std::fs::write(disk.join("DiskDescriptor.xml"), b"descriptor").unwrap();
    let runner = FakeRunner {
        home: bundle.to_string_lossy().into_owned(),
    };

    let inactive_plan = plan_with_runner(
        &runner,
        &prlctl,
        &disk_tool,
        "vm-123",
        &bundle,
        &disk,
        123,
        inactive(),
    )
    .unwrap();
    let mut active_evidence = inactive();
    active_evidence.active = true;
    active_evidence.observed_pids = vec![4242];
    let active_plan = plan_with_runner(
        &runner,
        &prlctl,
        &disk_tool,
        "vm-123",
        &bundle,
        &disk,
        123,
        active_evidence,
    )
    .unwrap();

    assert_ne!(
        inactive_plan.plan_fingerprint, active_plan.plan_fingerprint,
        "the plan identity must bind the active-use evidence that changes its blockers"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_bundle_is_rejected_before_provider_commands() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let prlctl = temp.path().join("prlctl");
    let disk_tool = temp.path().join("prl_disk_tool");
    std::fs::write(&prlctl, b"fake").unwrap();
    std::fs::write(&disk_tool, b"fake").unwrap();
    let real = temp.path().join("Real.pvm");
    std::fs::create_dir_all(real.join("disk.hdd")).unwrap();
    let linked = temp.path().join("Linked.pvm");
    symlink(&real, &linked).unwrap();
    let runner = FakeRunner {
        home: real.to_string_lossy().into_owned(),
    };
    let error = plan_with_runner(
        &runner,
        &prlctl,
        &disk_tool,
        "vm-123",
        &linked,
        &linked.join("disk.hdd"),
        123,
        inactive(),
    )
    .unwrap_err();
    assert_eq!(error, "parallels-symlink-path-rejected");
}
