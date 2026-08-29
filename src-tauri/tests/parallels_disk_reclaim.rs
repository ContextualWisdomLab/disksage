use disksage_lib::git_worktree::GitWorktreeActiveUseEvidence;
#[cfg(not(target_os = "macos"))]
use disksage_lib::parallels_disk_reclaim::enforce_cli_platform;
use disksage_lib::parallels_disk_reclaim::{
    approve, execute_with_runner, plan_with_runner, validate_cli_argument_tokens,
    ParallelsCommandRunner, ProcessParallelsCommandRunner,
};
use std::path::Path;

struct FakeRunner {
    home: String,
}

struct SnapshotRunner(FakeRunner);

impl ParallelsCommandRunner for SnapshotRunner {
    fn run(&self, executable: &Path, args: &[&str], label: &str) -> Result<String, String> {
        if args.first() == Some(&"snapshot-list") {
            Ok(r#"[{"ID":"snapshot-1"}]"#.into())
        } else {
            self.0.run(executable, args, label)
        }
    }

    fn permits_injected_executables(&self) -> bool {
        true
    }
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
            Some("snapshot-list") => Ok("[]".into()),
            Some("compact") if args.contains(&"--info") => Ok(
                "Block size: 8\nTotal blocks: 30000\nAllocated blocks: 15000\nUsed blocks: 2712\n"
                    .into(),
            ),
            Some("compact") => Ok(String::new()),
            _ => Err("unexpected-command".into()),
        }
    }

    fn permits_injected_executables(&self) -> bool {
        true
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
fn stopped_vm_fake_cli_reports_exact_48_mib_and_authorizes_exact_execution() {
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
    assert!(plan.execution_available);
    assert!(plan.blockers.is_empty());
    assert!(plan.snapshots_absent);
    assert!(plan.exact_approval_phrase.is_some());
}

#[test]
fn exact_fresh_approval_executes_only_non_force_compact_boundary() {
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
        1_000,
        inactive(),
    )
    .unwrap();
    let phrase = plan.exact_approval_phrase.clone().unwrap();
    let mut tampered = plan.clone();
    tampered.physical_bytes += 1;
    assert_eq!(
        approve(
            &tampered,
            &phrase,
            1_001,
            "human:test",
            "VM backup verified"
        )
        .unwrap_err(),
        "parallels-plan-integrity-mismatch"
    );
    let approval = approve(&plan, &phrase, 1_001, "human:test", "VM backup verified").unwrap();

    let result = execute_with_runner(
        &runner,
        &prlctl,
        &disk_tool,
        &plan,
        &approval,
        &phrase,
        1_002,
        inactive(),
    )
    .unwrap();

    assert!(result.execution_succeeded);
    assert_eq!(
        result.command,
        ["prl_disk_tool", "compact", "-hdd", "<approved-disk>"]
    );
    assert!(!result.command.iter().any(|argument| argument == "--force"));
}

#[test]
fn snapshots_and_expired_approval_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    let prlctl = root.join("prlctl");
    let disk_tool = root.join("prl_disk_tool");
    std::fs::write(&prlctl, b"fake").unwrap();
    std::fs::write(&disk_tool, b"fake").unwrap();
    let bundle = root.join("Work Windows.pvm");
    let disk = bundle.join("Work Windows-0.hdd");
    std::fs::create_dir_all(&disk).unwrap();
    let base = FakeRunner {
        home: bundle.to_string_lossy().into_owned(),
    };
    let snapshot_plan = plan_with_runner(
        &SnapshotRunner(base),
        &prlctl,
        &disk_tool,
        "vm-123",
        &bundle,
        &disk,
        1_000,
        inactive(),
    )
    .unwrap();
    assert!(!snapshot_plan.execution_available);
    assert!(snapshot_plan.exact_approval_phrase.is_none());

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
        1_000,
        inactive(),
    )
    .unwrap();
    let phrase = plan.exact_approval_phrase.clone().unwrap();
    let error = approve(&plan, &phrase, 301_001, "human:test", "VM backup verified").unwrap_err();
    assert_eq!(error, "parallels-plan-stale");
}

#[test]
fn production_runner_rejects_caller_selected_executables() {
    let temp = tempfile::tempdir().unwrap();
    let prlctl = temp.path().join("prlctl");
    let disk_tool = temp.path().join("prl_disk_tool");
    std::fs::write(&prlctl, b"fake").unwrap();
    std::fs::write(&disk_tool, b"fake").unwrap();
    let error = plan_with_runner(
        &ProcessParallelsCommandRunner,
        &prlctl,
        &disk_tool,
        "vm-123",
        temp.path(),
        temp.path(),
        1,
        inactive(),
    )
    .unwrap_err();
    assert_eq!(error, "parallels-command-unavailable-or-untrusted");
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

#[test]
fn cli_argument_contract_rejects_unknown_tokens() {
    let args = [
        "--vm-id",
        "vm-123",
        "--bundle",
        "/Users/example/Work Windows.pvm",
        "--disk",
        "/Users/example/Work Windows.pvm/Work Windows-0.hdd",
        "--bundel",
        "typo",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();

    let error = validate_cli_argument_tokens(&args).unwrap_err();

    assert_eq!(error, "지원하지 않는 인자가 있습니다: --bundel");
}

#[test]
fn plan_fingerprint_binds_observation_time() {
    let temp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    let prlctl = root.join("prlctl");
    let disk_tool = root.join("prl_disk_tool");
    std::fs::write(&prlctl, b"fake").unwrap();
    std::fs::write(&disk_tool, b"fake").unwrap();
    let bundle = root.join("Work Windows.pvm");
    let disk = bundle.join("Work Windows-0.hdd");
    std::fs::create_dir_all(&disk).unwrap();
    let runner = FakeRunner {
        home: bundle.to_string_lossy().into_owned(),
    };
    let first = plan_with_runner(
        &runner,
        &prlctl,
        &disk_tool,
        "vm-123",
        &bundle,
        &disk,
        1,
        inactive(),
    )
    .unwrap();
    let second = plan_with_runner(
        &runner,
        &prlctl,
        &disk_tool,
        "vm-123",
        &bundle,
        &disk,
        2,
        inactive(),
    )
    .unwrap();
    assert_ne!(first.plan_fingerprint, second.plan_fingerprint);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn parallels_cli_rejects_unsupported_host_platforms() {
    let error = enforce_cli_platform().unwrap_err();
    assert_eq!(
        error,
        "Parallels 디스크 회수 계획은 macOS에서만 지원합니다."
    );
}

#[cfg(unix)]
#[test]
fn symlinked_bundle_is_rejected_before_provider_commands() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(temp.path()).unwrap();
    let prlctl = root.join("prlctl");
    let disk_tool = root.join("prl_disk_tool");
    std::fs::write(&prlctl, b"fake").unwrap();
    std::fs::write(&disk_tool, b"fake").unwrap();
    let real = root.join("Real.pvm");
    std::fs::create_dir_all(real.join("disk.hdd")).unwrap();
    let linked = root.join("Linked.pvm");
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
