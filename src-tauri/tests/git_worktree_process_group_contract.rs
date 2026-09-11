#![cfg(unix)]

use disksage_lib::git_worktree::active_use_evidence;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct PathGuard(Option<OsString>);

impl Drop for PathGuard {
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
    }
}

#[test]
fn successful_helper_settles_descendants_before_bounded_reader_join() {
    let temporary = tempfile::tempdir().expect("temporary process fixture");
    let fake_bin = temporary.path().join("bin");
    let artifact = temporary.path().join("artifact");
    fs::create_dir_all(&fake_bin).expect("fake binary directory");
    fs::create_dir(&artifact).expect("artifact directory");

    let fake_lsof = fake_bin.join("lsof");
    fs::write(
        &fake_lsof,
        b"#!/bin/sh\nsleep 5 &\nprintf 'p999999\\0'\nexit 0\n",
    )
    .expect("write fake lsof");
    fs::set_permissions(&fake_lsof, fs::Permissions::from_mode(0o755))
        .expect("make fake lsof executable");

    let original_path = std::env::var_os("PATH");
    let _guard = PathGuard(original_path.clone());
    let mut paths = vec![PathBuf::from(&fake_bin)];
    if let Some(value) = original_path.as_ref() {
        paths.extend(std::env::split_paths(value));
    }
    std::env::set_var(
        "PATH",
        std::env::join_paths(paths).expect("construct fixture PATH"),
    );

    let started = Instant::now();
    let _ = active_use_evidence(&artifact, 250, 8, false);
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(2),
        "successful direct-child exit must not let a descendant retain stdout past the bounded command lifecycle; elapsed={elapsed:?}"
    );
}
