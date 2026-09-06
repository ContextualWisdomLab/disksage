// Exercise the real staging implementation with a controlled activity boundary.
// This verifies dispatch and restoration, not native lsof/ps process coverage.
#[path = "../src/generated_cache_reclaim.rs"]
mod generated_cache_reclaim;

mod rules {
    pub fn shared_temp_root() -> std::path::PathBuf {
        std::env::temp_dir()
    }
}

mod git_worktree {
    use std::{
        cell::RefCell,
        path::{Path, PathBuf},
    };

    #[derive(Default)]
    struct Probe {
        original: PathBuf,
        activate_at: usize,
        calls: usize,
        parent_calls: usize,
        parent_fault_at: usize,
        incomplete_parent: bool,
    }
    thread_local! { static PROBE: RefCell<Probe> = RefCell::new(Probe::default()); }

    pub struct ActiveUseEvidence {
        pub assessed: bool,
        pub evidence_complete: bool,
        pub observed_pids: Vec<u32>,
        pub active: bool,
    }

    pub fn configure(original: &Path, activate_at: usize) {
        PROBE.with(|probe| {
            *probe.borrow_mut() = Probe {
                original: original.to_owned(),
                activate_at,
                calls: 0,
                ..Probe::default()
            }
        });
    }

    pub fn parent_fault(at: usize, incomplete: bool) {
        PROBE.with(|probe| {
            *probe.borrow_mut() = Probe {
                parent_fault_at: at,
                incomplete_parent: incomplete,
                ..Probe::default()
            }
        });
    }

    pub fn calls() -> usize {
        PROBE.with(|probe| probe.borrow().calls)
    }

    pub fn active_use_evidence(
        path: &Path,
        timeout: u64,
        max_pids: usize,
        recursive: bool,
    ) -> ActiveUseEvidence {
        active_use_evidence_with_command_path(path, path, timeout, max_pids, recursive)
    }

    pub fn active_use_evidence_with_command_path(
        object: &Path,
        command: &Path,
        _: u64,
        _: usize,
        recursive: bool,
    ) -> ActiveUseEvidence {
        let active = PROBE.with(|probe| {
            let mut probe = probe.borrow_mut();
            if !recursive {
                probe.parent_calls += 1;
                return probe.parent_calls == probe.parent_fault_at && !probe.incomplete_parent;
            }
            probe.calls += 1;
            // A process starts mentioning the original path only after staging.
            let staged = object != probe.original && object.exists() && !probe.original.exists();
            staged && recursive && probe.calls == probe.activate_at && command == probe.original
        });
        ActiveUseEvidence {
            assessed: true,
            evidence_complete: PROBE.with(|probe| {
                let probe = probe.borrow();
                recursive || !probe.incomplete_parent || probe.parent_calls != probe.parent_fault_at
            }),
            observed_pids: if active { vec![42] } else { vec![] },
            active,
        }
    }
}

#[cfg(unix)]
fn assert_original_command_activity_restores_cache(activate_at: usize) {
    use generated_cache_reclaim::*;
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = home.join(".cache/torch");
    std::fs::create_dir_all(&root).unwrap();
    let content = b"preserve this generated model";
    std::fs::write(root.join("model.bin"), content).unwrap();
    let plan = plan_with_evidence(
        &root,
        home,
        GeneratedCacheActivityEvidence {
            evidence_complete: true,
            open_pids: vec![],
            tool_lock_paths: vec![],
            live_cwd_present: false,
            git_common_dir: None,
            git_worktree_registered: false,
            git_dirty: false,
        },
        1,
    )
    .unwrap();
    git_worktree::configure(&root, activate_at);
    let result = stage_and_remove_regenerable_root(&plan, &root, home, 2, u64::MAX);
    assert_eq!(result.unwrap_err(), "generated-cache-staged-active-use");
    assert_eq!(git_worktree::calls(), activate_at);
    assert_eq!(std::fs::read(root.join("model.bin")).unwrap(), content);
    assert_eq!(
        std::fs::read_dir(root.parent().unwrap()).unwrap().count(),
        1
    );
    git_worktree::configure(Path::new(""), 0);
}

use std::path::Path;

#[cfg(unix)]
#[test]
fn original_command_activity_at_first_staged_probe_restores_cache() {
    assert_original_command_activity_restores_cache(3);
}

#[cfg(unix)]
#[test]
fn original_command_activity_after_staged_hash_restores_cache() {
    assert_original_command_activity_restores_cache(4);
}

#[cfg(unix)]
fn fixture_plan(root: &Path, home: &Path) -> generated_cache_reclaim::GeneratedCachePlan {
    use generated_cache_reclaim::*;
    plan_with_evidence(
        root,
        home,
        GeneratedCacheActivityEvidence {
            evidence_complete: true,
            open_pids: vec![],
            tool_lock_paths: vec![],
            live_cwd_present: false,
            git_common_dir: None,
            git_worktree_registered: false,
            git_dirty: false,
        },
        1,
    )
    .unwrap()
}

#[cfg(unix)]
#[test]
fn parent_holder_and_incomplete_probe_preserve_cache_at_every_boundary() {
    for incomplete in [false, true] {
        for boundary in 1..=4 {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join(".cache/torch");
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("model.bin"), b"retained").unwrap();
            let plan = fixture_plan(&root, temp.path());
            git_worktree::parent_fault(boundary, incomplete);
            let result = generated_cache_reclaim::stage_and_remove_regenerable_root(
                &plan,
                &root,
                temp.path(),
                2,
                u64::MAX,
            );
            assert_eq!(
                result.unwrap_err(),
                if incomplete {
                    "generated-cache-parent-activity-incomplete"
                } else {
                    "generated-cache-parent-active-use"
                }
            );
            assert_eq!(std::fs::read(root.join("model.bin")).unwrap(), b"retained");
            assert_eq!(
                std::fs::read_dir(root.parent().unwrap()).unwrap().count(),
                1
            );
            git_worktree::configure(Path::new(""), 0);
        }
    }
}

#[cfg(unix)]
#[test]
fn replaced_parent_changes_plan_and_rejects_original_approval() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join(".cache");
    let root = parent.join("torch");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("model.bin"), b"retained").unwrap();
    let plan = fixture_plan(&root, temp.path());
    let displaced = temp.path().join("displaced-parent");
    std::fs::rename(&parent, &displaced).unwrap();
    std::fs::create_dir(&parent).unwrap();
    std::fs::rename(displaced.join("torch"), &root).unwrap();
    let fresh = fixture_plan(&root, temp.path());
    assert_ne!(plan.immediate_parent, fresh.immediate_parent);
    assert_eq!(plan.content_fingerprint, fresh.content_fingerprint);
    assert_ne!(plan.plan_fingerprint, fresh.plan_fingerprint);
    assert_eq!(
        generated_cache_reclaim::stage_and_remove_regenerable_root(
            &plan,
            &root,
            temp.path(),
            2,
            u64::MAX
        )
        .unwrap_err(),
        "generated-cache-parent-identity-changed"
    );
    assert_eq!(std::fs::read(root.join("model.bin")).unwrap(), b"retained");
}

#[cfg(unix)]
#[test]
fn stale_schema_and_symlink_parent_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".cache/torch");
    std::fs::create_dir_all(&root).unwrap();
    let mut plan = fixture_plan(&root, temp.path());
    plan.schema_version = 1;
    assert!(generated_cache_reclaim::approve(
        &plan,
        &plan.exact_approval_phrase,
        "reviewer",
        "fixture",
        2
    )
    .is_err());
    assert!(generated_cache_reclaim::stage_and_remove_regenerable_root(
        &plan,
        &root,
        temp.path(),
        2,
        u64::MAX
    )
    .is_err());
    #[cfg(unix)]
    {
        std::fs::rename(root.parent().unwrap(), temp.path().join("real-parent")).unwrap();
        std::os::unix::fs::symlink(temp.path().join("real-parent"), root.parent().unwrap())
            .unwrap();
        assert!(generated_cache_reclaim::audit(&root, temp.path(), 3)
            .unwrap_err()
            .contains("parent-not-real-directory"));
    }
}
