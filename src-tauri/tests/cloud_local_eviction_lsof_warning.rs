#![cfg(all(unix, not(coverage)))]

use disksage_lib::cloud_local_eviction::{observe_path_active_use, observe_path_active_use_until};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct PathGuard(Option<OsString>);

impl Drop for PathGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            std::env::set_var("PATH", path);
        } else {
            std::env::remove_var("PATH");
        }
    }
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("write fake executable");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod fake executable");
}

fn with_fake_tools(lsof_body: &str, target: &Path) -> disksage_lib::cloud_local_eviction::ActiveUseEvidence {
    let _lock = ENV_LOCK.lock().expect("serialize PATH mutation");
    let tools = tempfile::tempdir().expect("fake tool directory");
    write_executable(&tools.path().join("lsof"), lsof_body);
    write_executable(&tools.path().join("ps"), "#!/bin/sh\nexit 0\n");

    let previous = std::env::var_os("PATH");
    let _guard = PathGuard(previous.clone());
    let mut paths = vec![tools.path().to_path_buf()];
    if let Some(previous) = previous {
        paths.extend(std::env::split_paths(&previous));
    }
    std::env::set_var("PATH", std::env::join_paths(paths).expect("compose PATH"));

    observe_path_active_use(target)
}

fn with_fake_tools_until(
    lsof_body: &str,
    ps_body: &str,
    target: &Path,
    timeout: Duration,
) -> disksage_lib::cloud_local_eviction::ActiveUseEvidence {
    let _lock = ENV_LOCK.lock().expect("serialize PATH mutation");
    let tools = tempfile::tempdir().expect("fake tool directory");
    write_executable(&tools.path().join("lsof"), lsof_body);
    write_executable(&tools.path().join("ps"), ps_body);

    let previous = std::env::var_os("PATH");
    let _guard = PathGuard(previous.clone());
    let mut paths = vec![tools.path().to_path_buf()];
    if let Some(previous) = previous {
        paths.extend(std::env::split_paths(&previous));
    }
    std::env::set_var("PATH", std::env::join_paths(paths).expect("compose PATH"));

    observe_path_active_use_until(target, Instant::now() + timeout)
}

#[test]
fn unrelated_lsof_mount_warning_does_not_invalidate_target_observation() {
    let temp = tempfile::tempdir().expect("target fixture");
    let target = temp.path().join("file.bin");
    fs::write(&target, b"fixture").expect("write target");
    let evidence = with_fake_tools(
        "#!/bin/sh\necho \"lsof: WARNING: can't stat() fuse.portal file system /Volumes/unrelated\" >&2\necho \"      Output information may be incomplete.\" >&2\nexit 1\n",
        &target,
    );

    assert!(evidence.evidence_complete, "{evidence:?}");
    assert!(!evidence.active);
    assert!(!evidence.results_truncated);
    assert!(evidence.error.is_none());
}

#[test]
fn lsof_warning_for_target_directory_remains_fail_closed() {
    let temp = tempfile::tempdir().expect("target fixture");
    let target = temp.path().join("cache");
    fs::create_dir(&target).expect("create target directory");
    let script = format!(
        "#!/bin/sh\necho \"lsof: WARNING: can't stat({}): Permission denied\" >&2\necho \"      Output information may be incomplete.\" >&2\nexit 1\n",
        target.display()
    );
    let evidence = with_fake_tools(&script, &target);

    assert!(!evidence.evidence_complete, "{evidence:?}");
    assert!(!evidence.active);
    assert!(evidence.error.is_some());
}

#[test]
fn slow_lsof_does_not_starve_the_process_command_probe() {
    let temp = tempfile::tempdir().expect("target fixture");
    let target = temp.path().join("cache-artifact-under-active-use");
    fs::create_dir(&target).expect("create target directory");
    let ps_script = format!(
        "#!/bin/sh\nsleep 0.05\nprintf '4242 1 /usr/bin/cat {}\\n'\n",
        target.display()
    );

    let evidence = with_fake_tools_until(
        "#!/bin/sh\nsleep 2\nexit 1\n",
        &ps_script,
        &target,
        Duration::from_millis(600),
    );

    assert!(evidence.active, "{evidence:?}");
    assert_eq!(evidence.observed_pids, vec![4242]);
    assert!(
        !evidence.evidence_complete,
        "the timed-out lsof probe must still make the combined evidence fail closed"
    );
}
