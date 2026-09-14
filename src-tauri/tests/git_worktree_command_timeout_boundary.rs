//! Production-boundary regression for Git worktree subprocess deadlines.
//!
//! A caller may have a long overall audit budget, but that budget must never become the deadline
//! for one local `git` subprocess. The fixture places a deliberately blocking `git` on PATH and
//! proves an oversized per-command deadline is rejected before that child can hold the audit open.

#![cfg(target_os = "linux")]

use disksage_lib::git_worktree::{audit_git_worktrees, GitWorktreeAuditOptions};
use std::{
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    sync::Mutex,
    time::{Duration, Instant},
};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvRestore {
    key: &'static str,
    value: Option<OsString>,
}

impl EnvRestore {
    fn capture(key: &'static str) -> Self {
        Self {
            key,
            value: std::env::var_os(key),
        }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        match self.value.take() {
            Some(value) => unsafe { std::env::set_var(self.key, value) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

#[test]
fn hour_scale_budget_is_rejected_before_blocking_git_subprocess_starts() {
    let _guard = ENV_LOCK.lock().expect("environment test lock");
    let path_restore = EnvRestore::capture("PATH");

    let fixture = tempfile::tempdir().expect("fixture directory");
    let bin_dir = fixture.path().join("bin");
    let repository = fixture.path().join("repository");
    fs::create_dir(&bin_dir).expect("bin directory");
    fs::create_dir(&repository).expect("repository directory");

    let fake_git = bin_dir.join("git");
    fs::write(&fake_git, "#!/bin/sh\nsleep 1\nexit 0\n").expect("blocking git fixture");
    fs::set_permissions(&fake_git, fs::Permissions::from_mode(0o700)).expect("git executable mode");
    unsafe { std::env::set_var("PATH", &bin_dir) };

    let options = GitWorktreeAuditOptions {
        command_timeout_ms: 3_600_000,
        ..GitWorktreeAuditOptions::default()
    };
    let started = Instant::now();
    let result = audit_git_worktrees(&repository, &["HEAD".into()], options, 7_001);
    let elapsed = started.elapsed();
    drop(path_restore);

    assert_eq!(
        result.unwrap_err(),
        "git-worktree-command-timeout-out-of-bounds"
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "oversized local command budget reached the blocking git child: {elapsed:?}"
    );
}
