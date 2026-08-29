#[cfg(unix)]
mod unix {
    use disksage_lib::git_worktree::active_use_evidence;
    use std::process::Command;

    fn spawn_holder(argument: &str) -> std::process::Child {
        Command::new("sh")
            .args(["-c", "sleep 20 & wait", "disksage-option-path", argument])
            .spawn()
            .expect("spawn option-path holder")
    }

    #[test]
    fn option_assignment_paths_are_detected_without_matching_longer_siblings() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("target");
        std::fs::create_dir(&target).expect("target directory");

        let exact_argument = format!("--cwd={}", target.display());
        let mut exact = spawn_holder(&exact_argument);
        let exact_evidence = active_use_evidence(&target, 5_000, 64, true);
        let _ = exact.kill();
        let _ = exact.wait();
        assert!(exact_evidence.evidence_complete, "{exact_evidence:?}");
        assert!(exact_evidence.active, "{exact_evidence:?}");
        assert!(
            exact_evidence.observed_pids.contains(&exact.id()),
            "{exact_evidence:?}"
        );

        let descendant = target.join("child");
        let descendant_argument = format!("--cache={}", descendant.display());
        let mut recursive = spawn_holder(&descendant_argument);
        let recursive_evidence = active_use_evidence(&target, 5_000, 64, true);
        let _ = recursive.kill();
        let _ = recursive.wait();
        assert!(recursive_evidence.evidence_complete, "{recursive_evidence:?}");
        assert!(recursive_evidence.active, "{recursive_evidence:?}");
        assert!(
            recursive_evidence.observed_pids.contains(&recursive.id()),
            "{recursive_evidence:?}"
        );

        let sibling_argument = format!("--cwd={}-old", target.display());
        let mut sibling = spawn_holder(&sibling_argument);
        let sibling_evidence = active_use_evidence(&target, 5_000, 64, true);
        let _ = sibling.kill();
        let _ = sibling.wait();
        assert!(sibling_evidence.evidence_complete, "{sibling_evidence:?}");
        assert!(!sibling_evidence.active, "{sibling_evidence:?}");
    }
}
