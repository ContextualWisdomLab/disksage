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
            probe.calls += 1;
            // A process starts mentioning the original path only after staging.
            let staged = object != probe.original && object.exists() && !probe.original.exists();
            staged && recursive && probe.calls == probe.activate_at && command == probe.original
        });
        ActiveUseEvidence {
            assessed: true,
            evidence_complete: true,
            observed_pids: if active { vec![42] } else { vec![] },
            active,
        }
    }
}

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

#[test]
fn original_command_activity_at_first_staged_probe_restores_cache() {
    assert_original_command_activity_restores_cache(3);
}

#[test]
fn original_command_activity_after_staged_hash_restores_cache() {
    assert_original_command_activity_restores_cache(4);
}
