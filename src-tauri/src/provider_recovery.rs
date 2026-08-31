//! Bounded restart requests for desktop provider clients.
//!
//! This is a recovery action, not a copy or eviction authority.  It only targets the two
//! user-space desktop clients whose process names are already observed by `provider_client_runtime`.

use crate::cloud::CloudProvider;
use serde::{Deserialize, Serialize};

pub const PROVIDER_RECOVERY_SCHEMA_VERSION: u32 = 1;
const FINDER_COPY_CANCEL_SCRIPT: &str = "tell application \"Finder\" to activate\ntell application \"System Events\" to tell process \"Finder\" to key code 53\n";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderRecoveryOutput {
    pub schema_version: u32,
    pub provider: CloudProvider,
    pub action: String,
    pub pre_runtime_observed: bool,
    pub quit_requested: bool,
    pub launch_requested: bool,
    pub post_runtime_observed: Option<bool>,
    pub blockers: Vec<String>,
    pub cloud_write_executed: bool,
    pub source_eviction_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OneDriveUnpinOutcome {
    pub restart_blockers: Vec<String>,
}

pub fn recovery_supported(provider: CloudProvider) -> bool {
    matches!(
        provider,
        CloudProvider::Onedrive | CloudProvider::GoogleDrive
    )
}

fn post_runtime_blockers(runtime_observed: Option<bool>) -> Vec<String> {
    match runtime_observed {
        Some(true) => Vec::new(),
        Some(false) => vec!["provider-client-runtime-not-observed-after-restart".into()],
        None => vec!["provider-client-runtime-evidence-unavailable-after-restart".into()],
    }
}

fn recovery_output_after_launch(
    provider: CloudProvider,
    pre_runtime_observed: bool,
    allow_graceful_term: bool,
    post_runtime_observed: Option<bool>,
) -> ProviderRecoveryOutput {
    ProviderRecoveryOutput {
        schema_version: PROVIDER_RECOVERY_SCHEMA_VERSION,
        provider,
        action: if allow_graceful_term {
            "restart-provider-client-with-graceful-term".into()
        } else {
            "restart-provider-client".into()
        },
        pre_runtime_observed,
        quit_requested: true,
        launch_requested: true,
        post_runtime_observed,
        blockers: post_runtime_blockers(post_runtime_observed),
        cloud_write_executed: false,
        source_eviction_executed: false,
    }
}

fn finish_onedrive_unpin(
    operation: Result<(), String>,
    restart: Result<(), String>,
) -> Result<OneDriveUnpinOutcome, String> {
    operation?;
    Ok(OneDriveUnpinOutcome {
        restart_blockers: restart.err().into_iter().collect(),
    })
}

fn ensure_onedrive_stop_authority(
    primary_runtime_observed: bool,
    current_runtime_observed: bool,
) -> Result<(), String> {
    if !primary_runtime_observed && current_runtime_observed {
        Err("provider-recovery-runtime-started-concurrently".into())
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OneDriveQuitWaitDecision {
    Stopped,
    ContinueWaiting,
    TimedOut,
}

fn onedrive_quit_wait_decision(
    current_runtime_observed: bool,
    deadline_reached: bool,
) -> OneDriveQuitWaitDecision {
    if !current_runtime_observed {
        OneDriveQuitWaitDecision::Stopped
    } else if deadline_reached {
        OneDriveQuitWaitDecision::TimedOut
    } else {
        OneDriveQuitWaitDecision::ContinueWaiting
    }
}

/// Request Finder to cancel its active copy/materialization dialog without touching any provider
/// daemon, cloud object, or source file. The fixed AppleScript sends only Escape; it accepts no
/// user-provided script, path, or process identifier.
#[cfg(not(coverage))]
#[tauri::command]
pub fn cancel_finder_copy() -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    {
        return Err("finder-copy-cancel-platform-unsupported".into());
    }
    #[cfg(target_os = "macos")]
    {
        let ok = run_bounded(
            Path::new("/usr/bin/osascript"),
            &["-e", FINDER_COPY_CANCEL_SCRIPT],
        )
        .map_err(|error| {
            match error.as_str() {
                "provider-recovery-command-spawn-failed" => "finder-copy-cancel-spawn-failed",
                "provider-recovery-command-timeout" => "finder-copy-cancel-timeout",
                "provider-recovery-command-wait-failed" => "finder-copy-cancel-wait-failed",
                _ => "finder-copy-cancel-command-failed",
            }
            .to_string()
        })?;
        if ok {
            Ok(())
        } else {
            Err("finder-copy-cancel-command-failed".into())
        }
    }
}

#[cfg(not(coverage))]
use std::path::{Path, PathBuf};
#[cfg(not(coverage))]
use std::process::{Command, Stdio};
#[cfg(not(coverage))]
use std::time::{Duration, Instant};

#[cfg(not(coverage))]
fn app_spec(provider: CloudProvider) -> Option<(&'static str, &'static str)> {
    match provider {
        CloudProvider::Onedrive => Some(("OneDrive", "com.microsoft.OneDrive")),
        CloudProvider::GoogleDrive => Some(("Google Drive", "com.google.drivefs")),
        CloudProvider::Icloud => None,
    }
}

#[cfg(not(coverage))]
fn app_name(provider: CloudProvider) -> Option<&'static str> {
    app_spec(provider).map(|(name, _)| name)
}

#[cfg(not(coverage))]
#[cfg(target_os = "macos")]
fn verified_bundle(path: &Path, expected_bundle_id: &str) -> bool {
    let info = path.join("Contents/Info.plist");
    let Ok(metadata) = std::fs::symlink_metadata(&info) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return false;
    }
    let Ok(plist) = plist::Value::from_file(&info) else {
        return false;
    };
    plist
        .as_dictionary()
        .and_then(|dictionary| dictionary.get("CFBundleIdentifier"))
        .and_then(plist::Value::as_string)
        == Some(expected_bundle_id)
}

#[cfg(not(coverage))]
#[cfg(not(target_os = "macos"))]
fn verified_bundle(_path: &Path, _expected_bundle_id: &str) -> bool {
    false
}

#[cfg(not(coverage))]
fn app_path(provider: CloudProvider) -> Result<PathBuf, String> {
    let (name, bundle_id) =
        app_spec(provider).ok_or_else(|| "provider-recovery-system-managed".to_string())?;
    let mut candidates = vec![PathBuf::from("/Applications").join(format!("{name}.app"))];
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(
            PathBuf::from(home)
                .join("Applications")
                .join(format!("{name}.app")),
        );
    }
    candidates
        .into_iter()
        .find(|path| {
            std::fs::symlink_metadata(path)
                .map(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
                .unwrap_or(false)
                && verified_bundle(path, bundle_id)
        })
        .ok_or_else(|| "provider-recovery-client-app-not-found".to_string())
}

#[cfg(all(target_os = "macos", not(coverage)))]
pub(crate) fn onedrive_files_on_demand_available() -> bool {
    app_path(CloudProvider::Onedrive)
        .map(|app| app.join("Contents/MacOS/OneDrive").is_file())
        .unwrap_or(false)
}

#[cfg(not(coverage))]
fn run_bounded(program: &Path, args: &[&str]) -> Result<bool, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "provider-recovery-command-spawn-failed".to_string())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.success()),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("provider-recovery-command-timeout".into());
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("provider-recovery-command-wait-failed".into());
            }
        }
    }
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn onedrive_command_succeeded(status_success: bool, output: &[u8]) -> bool {
    status_success
        && !output
            .windows(b"Failed operation=".len())
            .any(|window| window == b"Failed operation=")
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn run_bounded_output(program: &Path, args: &[&str]) -> Result<(), String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut capture = tempfile::tempfile()
        .map_err(|_| "provider-recovery-command-output-unavailable".to_string())?;
    let stderr = capture
        .try_clone()
        .map_err(|_| "provider-recovery-command-output-unavailable".to_string())?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(capture.try_clone().map_err(|_| {
            "provider-recovery-command-output-unavailable".to_string()
        })?))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|_| "provider-recovery-command-spawn-failed".to_string())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("provider-recovery-command-timeout".into());
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("provider-recovery-command-wait-failed".into());
            }
        }
    };
    capture
        .seek(SeekFrom::Start(0))
        .map_err(|_| "provider-recovery-command-output-unavailable".to_string())?;
    let mut output = Vec::new();
    capture
        .take(64 * 1024 + 1)
        .read_to_end(&mut output)
        .map_err(|_| "provider-recovery-command-output-unavailable".to_string())?;
    if output.len() > 64 * 1024 {
        return Err("provider-recovery-command-output-too-large".into());
    }
    if onedrive_command_succeeded(status.success(), &output) {
        Ok(())
    } else {
        Err("onedrive-files-on-demand-command-failed".into())
    }
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn launch_provider(path: &Path) -> Result<(), String> {
    let path = path
        .to_str()
        .ok_or_else(|| "provider-recovery-client-path-invalid".to_string())?;
    if !run_bounded(Path::new("/usr/bin/open"), &["-a", path])? {
        return Err("provider-recovery-launch-failed".into());
    }
    Ok(())
}

/// Invoke OneDrive's documented Files On-Demand command while its sync app is stopped, then
/// restore the verified app only when it was running before the maintenance operation.
#[cfg(all(target_os = "macos", not(coverage)))]
pub(crate) fn unpin_onedrive_local_copy(path: &Path) -> Result<OneDriveUnpinOutcome, String> {
    let app = app_path(CloudProvider::Onedrive)?;
    let executable = app.join("Contents/MacOS/OneDrive");
    if !executable.is_file() {
        return Err("onedrive-files-on-demand-command-unavailable".into());
    }
    let path = path
        .to_str()
        .ok_or_else(|| "cloud-local-eviction-path-not-unicode".to_string())?;
    let primary_runtime_observed = crate::provider_client_runtime::collect_provider_primary_runtime(
        CloudProvider::Onedrive,
    )
    .ok_or_else(|| "provider-recovery-runtime-evidence-unavailable".to_string())?;
    if primary_runtime_observed {
        request_quit("OneDrive")?;
    }
    let operation = (|| {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let current_runtime_observed = require_primary_runtime_observation(CloudProvider::Onedrive)?;
            ensure_onedrive_stop_authority(primary_runtime_observed, current_runtime_observed)?;
            match onedrive_quit_wait_decision(
                current_runtime_observed,
                Instant::now() >= deadline,
            ) {
                OneDriveQuitWaitDecision::Stopped => break,
                OneDriveQuitWaitDecision::ContinueWaiting => {}
                OneDriveQuitWaitDecision::TimedOut => {
                    return Err("provider-recovery-quit-timeout".into());
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        run_bounded_output(&executable, &["/unpin", path])
    })();
    let restart = crate::provider_runtime_state::restore_after_temporary_stop(
        primary_runtime_observed,
        || {
            launch_provider(&app).and_then(|_| {
                std::thread::sleep(Duration::from_secs(1));
                match runtime_observation(CloudProvider::Onedrive, 0) {
                    Some(true) => Ok(()),
                    Some(false) => {
                        Err("provider-client-runtime-not-observed-after-restart".into())
                    }
                    None => Err("provider-client-runtime-evidence-unavailable-after-restart".into()),
                }
            })
        },
    );
    finish_onedrive_unpin(operation, restart)
}

#[cfg(not(coverage))]
fn runtime_observation(provider: CloudProvider, observed_at_ms: u64) -> Option<bool> {
    crate::provider_client_runtime::collect_provider_client_runtime(provider, observed_at_ms)
        .runtime_observed
}

#[cfg(not(coverage))]
fn require_runtime_observation(
    provider: CloudProvider,
    observed_at_ms: u64,
) -> Result<bool, String> {
    runtime_observation(provider, observed_at_ms)
        .ok_or_else(|| "provider-recovery-runtime-evidence-unavailable".to_string())
}

#[cfg(not(coverage))]
fn require_primary_runtime_observation(provider: CloudProvider) -> Result<bool, String> {
    crate::provider_client_runtime::collect_provider_primary_runtime(provider)
        .ok_or_else(|| "provider-recovery-runtime-evidence-unavailable".to_string())
}

#[cfg(not(coverage))]
fn request_quit(app: &str) -> Result<(), String> {
    // The app name is selected from the fixed provider map above; no user path or shell is parsed.
    let script = format!("tell application \"{app}\" to quit");
    let ok = run_bounded(Path::new("/usr/bin/osascript"), &["-e", script.as_str()])?;
    // AppleScript can return non-zero after the primary app has already disappeared. Extensions
    // may legitimately remain, so only the exact desktop process is authoritative here.
    let provider = if app == "OneDrive" {
        CloudProvider::Onedrive
    } else {
        CloudProvider::GoogleDrive
    };
    if !ok && require_primary_runtime_observation(provider)? {
        return Err("provider-recovery-quit-request-failed".into());
    }
    Ok(())
}

#[cfg(not(coverage))]
fn request_graceful_term(app: &str) -> Result<(), String> {
    // `app` comes only from the fixed provider map. SIGTERM is a graceful request; SIGKILL is
    // intentionally never used because an active desktop client may still be flushing state.
    let ok = run_bounded(Path::new("/usr/bin/killall"), &["-TERM", "--", app])?;
    let provider = if app == "OneDrive" {
        CloudProvider::Onedrive
    } else {
        CloudProvider::GoogleDrive
    };
    if !ok && require_primary_runtime_observation(provider)? {
        return Err("provider-recovery-graceful-term-failed".into());
    }
    Ok(())
}

#[cfg(not(coverage))]
/// Request a bounded restart of a user-space provider client.
///
/// iCloud is deliberately rejected because its daemon is system-managed.  A successful return
/// only proves that quit/launch requests completed; `post_runtime_observed` is the fresh local
/// process observation and never proves authentication, remote capacity, upload, or eviction.
pub fn recover_provider_client(
    provider: CloudProvider,
    observed_at_ms: u64,
) -> Result<ProviderRecoveryOutput, String> {
    recover_provider_client_with_options(provider, observed_at_ms, false)
}

#[cfg(not(coverage))]
/// Request a bounded provider restart, optionally escalating a failed AppleScript quit to an
/// explicit, graceful SIGTERM of the fixed verified desktop client.
pub fn recover_provider_client_with_options(
    provider: CloudProvider,
    observed_at_ms: u64,
    allow_graceful_term: bool,
) -> Result<ProviderRecoveryOutput, String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (provider, observed_at_ms);
        return Err("provider-recovery-platform-unsupported".into());
    }
    #[cfg(target_os = "macos")]
    {
        let app =
            app_name(provider).ok_or_else(|| "provider-recovery-system-managed".to_string())?;
        let path = app_path(provider)?;
        let pre_runtime_observed = require_runtime_observation(provider, observed_at_ms)?;
        if let Err(quit_error) = request_quit(app) {
            if !allow_graceful_term {
                return Err(quit_error);
            }
            request_graceful_term(app)?;
        }

        let quit_deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let runtime_observed = require_runtime_observation(provider, observed_at_ms)?;
            if !runtime_observed {
                break;
            }
            if Instant::now() >= quit_deadline {
                return Err("provider-recovery-quit-timeout".into());
            }
            std::thread::sleep(Duration::from_millis(250));
        }

        launch_provider(&path)?;
        std::thread::sleep(Duration::from_secs(1));
        let post_runtime_observed = runtime_observation(provider, observed_at_ms);
        Ok(recovery_output_after_launch(
            provider,
            pre_runtime_observed,
            allow_graceful_term,
            post_runtime_observed,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_user_space_clients_are_recoverable() {
        assert!(recovery_supported(CloudProvider::Onedrive));
        assert!(recovery_supported(CloudProvider::GoogleDrive));
        assert!(!recovery_supported(CloudProvider::Icloud));
    }

    #[test]
    fn finder_copy_cancel_script_keeps_two_statements() {
        assert_eq!(
            FINDER_COPY_CANCEL_SCRIPT.lines().collect::<Vec<_>>(),
            vec![
                "tell application \"Finder\" to activate",
                "tell application \"System Events\" to tell process \"Finder\" to key code 53",
            ]
        );
    }

    #[test]
    fn recovery_output_cannot_claim_data_mutation() {
        let json = serde_json::to_value(ProviderRecoveryOutput {
            schema_version: PROVIDER_RECOVERY_SCHEMA_VERSION,
            provider: CloudProvider::Onedrive,
            action: "restart-provider-client".into(),
            pre_runtime_observed: true,
            quit_requested: true,
            launch_requested: true,
            post_runtime_observed: Some(true),
            blockers: Vec::new(),
            cloud_write_executed: false,
            source_eviction_executed: false,
        })
        .unwrap();
        assert_eq!(json["cloud_write_executed"], false);
        assert_eq!(json["source_eviction_executed"], false);
    }

    #[test]
    fn slow_post_restart_observation_is_structured_recovery_evidence() {
        let output = recovery_output_after_launch(
            CloudProvider::Onedrive,
            true,
            false,
            Some(false),
        );
        assert_eq!(output.post_runtime_observed, Some(false));
        assert_eq!(
            output.blockers,
            vec!["provider-client-runtime-not-observed-after-restart"]
        );
        assert!(output.launch_requested);
        assert!(!output.cloud_write_executed);
        assert!(!output.source_eviction_executed);
    }

    #[test]
    fn unavailable_post_restart_observation_is_structured_recovery_evidence() {
        let output = recovery_output_after_launch(CloudProvider::GoogleDrive, true, true, None);
        assert_eq!(output.post_runtime_observed, None);
        assert_eq!(
            output.blockers,
            vec!["provider-client-runtime-evidence-unavailable-after-restart"]
        );
        assert_eq!(output.action, "restart-provider-client-with-graceful-term");
    }

    #[test]
    fn successful_unpin_preserves_restart_failure_as_a_blocker() {
        let outcome = finish_onedrive_unpin(
            Ok(()),
            Err("provider-client-runtime-not-observed-after-restart".into()),
        )
        .unwrap();
        assert_eq!(
            outcome.restart_blockers,
            vec!["provider-client-runtime-not-observed-after-restart"]
        );
    }

    #[test]
    fn failed_unpin_remains_a_hard_operation_failure() {
        assert_eq!(
            finish_onedrive_unpin(
                Err("onedrive-files-on-demand-command-failed".into()),
                Err("provider-client-runtime-not-observed-after-restart".into()),
            )
            .unwrap_err(),
            "onedrive-files-on-demand-command-failed"
        );
    }

    #[test]
    fn concurrently_started_onedrive_is_not_owned_by_maintenance_stop() {
        assert_eq!(
            ensure_onedrive_stop_authority(false, true).unwrap_err(),
            "provider-recovery-runtime-started-concurrently"
        );
        assert!(ensure_onedrive_stop_authority(false, false).is_ok());
        assert!(ensure_onedrive_stop_authority(true, true).is_ok());
    }

    #[test]
    fn onedrive_unpin_timeout_never_escalates_name_only_runtime_evidence() {
        assert_eq!(
            onedrive_quit_wait_decision(true, true),
            OneDriveQuitWaitDecision::TimedOut
        );
    }

    #[cfg(all(target_os = "macos", not(coverage)))]
    #[test]
    fn onedrive_command_rejects_failure_text_even_with_zero_exit() {
        assert!(onedrive_command_succeeded(true, b""));
        assert!(!onedrive_command_succeeded(
            true,
            b"Failed operation=2 status=-2"
        ));
        assert!(!onedrive_command_succeeded(false, b""));
    }

    #[cfg(all(not(target_os = "macos"), not(coverage)))]
    #[test]
    fn unavailable_runtime_evidence_is_not_process_absence() {
        assert_eq!(
            require_runtime_observation(CloudProvider::Onedrive, 0).unwrap_err(),
            "provider-recovery-runtime-evidence-unavailable"
        );
    }

    #[test]
    fn post_restart_blockers_preserve_unavailable_runtime_evidence() {
        assert!(post_runtime_blockers(Some(true)).is_empty());
        assert_eq!(
            post_runtime_blockers(Some(false)),
            vec!["provider-client-runtime-not-observed-after-restart"]
        );
        assert_eq!(
            post_runtime_blockers(None),
            vec!["provider-client-runtime-evidence-unavailable-after-restart"]
        );
    }

    #[cfg(all(not(target_os = "macos"), not(coverage)))]
    #[test]
    fn finder_copy_cancel_is_explicitly_unsupported_off_macos() {
        assert_eq!(
            cancel_finder_copy().unwrap_err(),
            "finder-copy-cancel-platform-unsupported"
        );
    }
}
