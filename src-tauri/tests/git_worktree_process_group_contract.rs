#![cfg(unix)]

use disksage_lib::git_worktree::active_use_evidence;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Restores the caller's PATH after the fake `lsof` process fixture completes.
struct PathGuard(Option<OsString>);

impl Drop for PathGuard {
    /// Restores the inherited environment so this PATH-mutating fixture cannot leak into later tests.
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
    }
}

/// Prepends a fixture directory to PATH for the duration of one process-boundary test.
fn prepend_fixture_path(fake_bin: &std::path::Path) -> PathGuard {
    let original_path = std::env::var_os("PATH");
    let guard = PathGuard(original_path.clone());
    let mut paths = vec![PathBuf::from(fake_bin)];
    if let Some(value) = original_path.as_ref() {
        paths.extend(std::env::split_paths(value));
    }
    std::env::set_var(
        "PATH",
        std::env::join_paths(paths).expect("construct fixture PATH"),
    );
    guard
}

/// Proves a successful helper cannot let a pipe-owning descendant outlive the command bound.
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

    let _guard = prepend_fixture_path(&fake_bin);
    let started = Instant::now();
    let evidence = active_use_evidence(&artifact, 250, 8, false);
    let elapsed = started.elapsed();

    assert!(evidence.assessed, "{evidence:?}");
    assert!(evidence.evidence_complete, "{evidence:?}");
    assert!(evidence.active, "{evidence:?}");
    assert_eq!(evidence.observed_pids, vec![999_999], "{evidence:?}");
    assert_eq!(evidence.error, None, "{evidence:?}");
    assert!(
        elapsed < Duration::from_secs(2),
        "successful direct-child exit must not let a descendant retain stdout past the bounded command lifecycle; elapsed={elapsed:?}"
    );
}

/// Reaps the direct helper externally so the production wait boundary receives a real `ECHILD`.
#[cfg(target_os = "linux")]
fn reap_fixture_leader(marker: PathBuf) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Ok(text) = fs::read_to_string(&marker) {
            if let Ok(pid) = text.trim().parse::<libc::pid_t>() {
                let mut status = 0;
                let reaped = unsafe { libc::waitpid(pid, &mut status, 0) };
                assert_eq!(reaped, pid, "fixture reaper must consume the direct helper");
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "fake lsof did not publish its leader PID"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Proves a non-interrupted wait observation failure releases DiskSage-owned pipe readers.
#[cfg(target_os = "linux")]
#[test]
fn observation_failure_does_not_detach_pipe_reader_fds() {
    let temporary = tempfile::tempdir().expect("temporary observation-error fixture");
    let fake_bin = temporary.path().join("bin");
    let artifact = temporary.path().join("artifact");
    let leader_marker = temporary.path().join("leader.pid");
    let pipe_marker = temporary.path().join("stdout.pipe");
    fs::create_dir_all(&fake_bin).expect("fake binary directory");
    fs::create_dir(&artifact).expect("artifact directory");

    let fake_lsof = fake_bin.join("lsof");
    let script = format!(
        "#!/bin/sh\nreadlink /proc/$$/fd/1 > '{}'\nprintf '%s\\n' \"$$\" > '{}'\nsleep 5 &\nsleep 0.2\nprintf 'p999999\\0'\nexit 0\n",
        pipe_marker.display(),
        leader_marker.display()
    );
    fs::write(&fake_lsof, script).expect("write observation-error lsof");
    fs::set_permissions(&fake_lsof, fs::Permissions::from_mode(0o755))
        .expect("make fake lsof executable");

    let _guard = prepend_fixture_path(&fake_bin);
    let reaper_marker = leader_marker.clone();
    let reaper = std::thread::spawn(move || reap_fixture_leader(reaper_marker));

    let started = Instant::now();
    let evidence = active_use_evidence(&artifact, 1_000, 8, false);
    let elapsed = started.elapsed();
    reaper.join().expect("external direct-child reaper");

    assert!(evidence.assessed, "{evidence:?}");
    assert!(!evidence.evidence_complete, "{evidence:?}");
    assert!(evidence.error.is_some(), "{evidence:?}");
    assert!(
        elapsed < Duration::from_secs(2),
        "observation failure must remain bounded; elapsed={elapsed:?}"
    );

    let pipe_identity = fs::read_to_string(&pipe_marker)
        .expect("stdout pipe marker")
        .trim()
        .to_string();
    assert!(pipe_identity.starts_with("pipe:["), "{pipe_identity}");
    let reader_still_owned = fs::read_dir("/proc/self/fd")
        .expect("process fd directory")
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_link(entry.path()).ok())
        .any(|target| target.to_string_lossy() == pipe_identity);
    assert!(
        !reader_still_owned,
        "non-interrupted observation failure must cancel and close DiskSage-owned reader FDs instead of detaching them; leaked={pipe_identity}"
    );
}
