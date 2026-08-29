//! Read-only planning and explicit guest trim for Podman and Colima storage.
//!
//! A VM-backed runtime can report a large logical store while its sparse host image keeps
//! allocated extents. DiskSage therefore separates guest `fstrim` from host-image compaction:
//! trim is an optional, bounded command; raw-image compaction is never guessed or run by the app.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const SCHEMA_VERSION: u32 = 2;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const RECOVERY_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_CAPTURE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeStorageKind {
    PodmanMachine,
    Colima,
}

impl RuntimeStorageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PodmanMachine => "podman-machine",
            Self::Colima => "colima",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeStoragePlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub runtime: RuntimeStorageKind,
    pub display_name: String,
    pub executable_available: bool,
    pub guest_running: Option<bool>,
    pub guest_reachable: Option<bool>,
    /// Fresh running-container count used only to authorize an inactive Podman machine stop.
    pub running_container_count: Option<u64>,
    pub trim_command: Option<Vec<String>>,
    pub stop_command: Option<Vec<String>>,
    pub recovery_command: Option<Vec<Vec<String>>>,
    pub host_compaction_supported: bool,
    pub host_compaction_blockers: Vec<String>,
    pub observed_at_ms: u64,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: Option<String>,
    pub stop_approval_phrase: Option<String>,
    pub recovery_approval_phrase: Option<String>,
    pub evidence_complete: bool,
    pub issue: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeStorageExecution {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub runtime: RuntimeStorageKind,
    pub command: Vec<String>,
    pub status_code: i32,
    #[serde(skip_serializing)]
    pub stdout: String,
    #[serde(skip_serializing)]
    pub stderr: String,
    pub output_truncated: bool,
    pub executed: bool,
    pub executed_at_ms: u64,
    pub rationale: String,
    pub volume_comparison: Option<crate::volume_pressure::LocalVolumeComparison>,
    pub volume_evidence_error: Option<String>,
    pub runtime_image_allocated_bytes_before: Option<u64>,
    pub runtime_image_allocated_bytes_after: Option<u64>,
    pub runtime_image_reclaimed_bytes: Option<u64>,
    pub runtime_image_evidence_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeStorageRecoveryExecution {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub runtime: RuntimeStorageKind,
    pub command: Vec<Vec<String>>,
    pub stop_status_code: i32,
    pub start_status_code: i32,
    pub guest_reachable_after_recovery: bool,
    pub executed: bool,
    pub executed_at_ms: u64,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeStorageStopExecution {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub runtime: RuntimeStorageKind,
    pub command: Vec<String>,
    pub status_code: i32,
    #[serde(skip_serializing)]
    pub stdout: String,
    #[serde(skip_serializing)]
    pub stderr: String,
    pub output_truncated: bool,
    pub running_container_count_before: u64,
    pub guest_running_after: Option<bool>,
    pub executed: bool,
    pub executed_at_ms: u64,
    pub rationale: String,
    pub volume_comparison: Option<crate::volume_pressure::LocalVolumeComparison>,
    pub volume_evidence_error: Option<String>,
    pub runtime_image_allocated_bytes_before: Option<u64>,
    pub runtime_image_allocated_bytes_after: Option<u64>,
    pub runtime_image_reclaimed_bytes: Option<u64>,
    pub runtime_image_evidence_error: Option<String>,
}

fn fixed_binary(runtime: RuntimeStorageKind) -> PathBuf {
    let (name, candidates): (&str, &[&str]) = match runtime {
        RuntimeStorageKind::PodmanMachine => (
            "podman",
            &[
                "/opt/homebrew/bin/podman",
                "/usr/local/bin/podman",
                "/usr/bin/podman",
            ],
        ),
        RuntimeStorageKind::Colima => (
            "colima",
            &[
                "/opt/homebrew/bin/colima",
                "/usr/local/bin/colima",
                "/usr/bin/colima",
            ],
        ),
    };
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|path| std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file()))
        .unwrap_or_else(|| PathBuf::from(name))
}

fn bounded_output(bytes: Vec<u8>) -> (String, bool) {
    let truncated = bytes.len() > MAX_CAPTURE_BYTES;
    let bytes = bytes
        .into_iter()
        .take(MAX_CAPTURE_BYTES)
        .collect::<Vec<_>>();
    (String::from_utf8_lossy(&bytes).into_owned(), truncated)
}

fn drain_bounded<R: Read>(mut reader: R) -> std::io::Result<(Vec<u8>, bool)> {
    let mut buffer = [0_u8; 8 * 1024];
    let mut captured = Vec::with_capacity(MAX_CAPTURE_BYTES);
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let room = MAX_CAPTURE_BYTES.saturating_sub(captured.len());
        captured.extend_from_slice(&buffer[..read.min(room)]);
        truncated |= read > room;
    }
    Ok((captured, truncated))
}

fn run_bounded(binary: &Path, args: &[&str]) -> Result<(i32, String, String, bool), String> {
    run_bounded_with_timeout(binary, args, COMMAND_TIMEOUT)
}

fn run_bounded_with_timeout(
    binary: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<(i32, String, String, bool), String> {
    let mut command = Command::new(binary);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|_| "runtime-storage-command-failed".to_string())?;
    let child_pid = child.id();
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("runtime-storage-stdout-pipe-unavailable".into());
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("runtime-storage-stderr-pipe-unavailable".into());
        }
    };
    let stdout_reader = thread::spawn(move || drain_bounded(stdout));
    let stderr_reader = thread::spawn(move || drain_bounded(stderr));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                #[cfg(unix)]
                unsafe {
                    let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err("runtime-storage-command-timeout".into());
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                #[cfg(unix)]
                unsafe {
                    let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err("runtime-storage-command-failed".into());
            }
        }
    };
    #[cfg(unix)]
    unsafe {
        let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
    }
    let (stdout, stdout_truncated) = stdout_reader
        .join()
        .map_err(|_| "runtime-storage-stdout-reader-panicked".to_string())?
        .map_err(|_| "runtime-storage-stdout-read-failed".to_string())?;
    let (stderr, stderr_truncated) = stderr_reader
        .join()
        .map_err(|_| "runtime-storage-stderr-reader-panicked".to_string())?
        .map_err(|_| "runtime-storage-stderr-read-failed".to_string())?;
    let (stdout, stdout_truncated_by_utf8) = bounded_output(stdout);
    let (stderr, stderr_truncated_by_utf8) = bounded_output(stderr);
    Ok((
        status.code().unwrap_or(-1),
        stdout,
        stderr,
        stdout_truncated
            || stderr_truncated
            || stdout_truncated_by_utf8
            || stderr_truncated_by_utf8,
    ))
}

fn reachability_from_probe(result: Result<(i32, String, String, bool), String>) -> Option<bool> {
    result.ok().map(|output| output.0 == 0)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn trim_command(runtime: RuntimeStorageKind) -> Vec<String> {
    match runtime {
        RuntimeStorageKind::PodmanMachine => vec![
            "podman".into(),
            "machine".into(),
            "ssh".into(),
            "podman-machine-default".into(),
            "--".into(),
            "sudo".into(),
            "fstrim".into(),
            "-av".into(),
        ],
        RuntimeStorageKind::Colima => vec![
            "colima".into(),
            "ssh".into(),
            "--".into(),
            "sudo".into(),
            "fstrim".into(),
            "-av".into(),
        ],
    }
}

fn fingerprint(
    runtime: RuntimeStorageKind,
    running: Option<bool>,
    reachable: Option<bool>,
    running_containers: Option<u64>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.runtime-storage-plan.v1\0");
    hasher.update(runtime.as_str().as_bytes());
    hasher.update([running.unwrap_or(false) as u8]);
    hasher.update([reachable.unwrap_or(false) as u8]);
    hasher.update(running_containers.unwrap_or(u64::MAX).to_le_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn inspect_runtime(runtime: RuntimeStorageKind, observed_at_ms: u64) -> RuntimeStoragePlan {
    let binary = fixed_binary(runtime);
    let version = run_bounded(&binary, &["--version"]);
    let executable_available = version.is_ok_and(|(status, _, _, _)| status == 0);
    let (guest_running, issue) = if !executable_available {
        (None, Some("runtime-storage-executable-unavailable".into()))
    } else {
        let state = match runtime {
            RuntimeStorageKind::PodmanMachine => run_bounded(
                &binary,
                &[
                    "machine",
                    "inspect",
                    "podman-machine-default",
                    "--format",
                    "{{.State}}",
                ],
            ),
            RuntimeStorageKind::Colima => run_bounded(&binary, &["status", "--json"]),
        };
        match state {
            Ok((status, stdout, _, _)) if status == 0 => {
                let (running, state_valid) = match runtime {
                    RuntimeStorageKind::PodmanMachine => {
                        (stdout.trim().eq_ignore_ascii_case("running"), true)
                    }
                    RuntimeStorageKind::Colima => {
                        let parsed = serde_json::from_str::<serde_json::Value>(&stdout).ok();
                        let status = parsed
                            .as_ref()
                            .and_then(|value| value.get("status"))
                            .and_then(serde_json::Value::as_str);
                        (
                            status.is_some_and(|value| value.eq_ignore_ascii_case("running")),
                            status.is_some(),
                        )
                    }
                };
                if runtime == RuntimeStorageKind::Colima && !state_valid {
                    (None, Some("runtime-storage-state-invalid".into()))
                } else {
                    (Some(running), None)
                }
            }
            Ok(_) => (None, Some("runtime-storage-state-unavailable".into())),
            Err(error) => (None, Some(error)),
        }
    };
    let guest_reachable = if guest_running == Some(true) {
        let args = match runtime {
            RuntimeStorageKind::PodmanMachine => {
                ["machine", "ssh", "podman-machine-default", "--", "true"].as_slice()
            }
            RuntimeStorageKind::Colima => ["ssh", "--", "true"].as_slice(),
        };
        // A completed non-zero probe proves the guest is unreachable. Probe failures and
        // timeouts are incomplete evidence and must not authorize a restart.
        reachability_from_probe(run_bounded(&binary, args))
    } else {
        None
    };
    let running_container_count = if runtime == RuntimeStorageKind::PodmanMachine
        && guest_running == Some(true)
        && guest_reachable == Some(true)
    {
        run_bounded(
            &binary,
            &[
                "--connection",
                "podman-machine-default",
                "ps",
                "--format",
                "json",
            ],
        )
        .ok()
        .filter(|output| output.0 == 0 && !output.3)
        .and_then(|output| serde_json::from_str::<serde_json::Value>(&output.1).ok())
        .and_then(|value| value.as_array().map(|records| records.len() as u64))
    } else {
        None
    };
    let fingerprint = fingerprint(
        runtime,
        guest_running,
        guest_reachable,
        running_container_count,
    );
    let ready =
        executable_available && guest_running == Some(true) && guest_reachable == Some(true);
    let recovery_ready =
        executable_available && guest_running == Some(true) && guest_reachable == Some(false);
    let recovery_command = recovery_ready.then(|| match runtime {
        RuntimeStorageKind::PodmanMachine => vec![
            vec![
                "podman".into(),
                "machine".into(),
                "stop".into(),
                "podman-machine-default".into(),
            ],
            vec![
                "podman".into(),
                "machine".into(),
                "start".into(),
                "podman-machine-default".into(),
            ],
        ],
        RuntimeStorageKind::Colima => vec![
            vec!["colima".into(), "stop".into()],
            vec!["colima".into(), "start".into()],
        ],
    });
    RuntimeStoragePlan {
        schema_kind: "disksage.runtime-storage-plan",
        schema_version: SCHEMA_VERSION,
        runtime,
        display_name: match runtime {
            RuntimeStorageKind::PodmanMachine => "Podman 가상 머신".into(),
            RuntimeStorageKind::Colima => "Colima 가상 머신".into(),
        },
        executable_available,
        guest_running,
        guest_reachable,
        running_container_count,
        trim_command: ready.then(|| trim_command(runtime)),
        stop_command: (runtime == RuntimeStorageKind::PodmanMachine
            && ready
            && running_container_count == Some(0))
        .then(|| {
            vec![
                "podman".into(),
                "machine".into(),
                "stop".into(),
                "podman-machine-default".into(),
            ]
        }),
        recovery_command,
        host_compaction_supported: false,
        host_compaction_blockers: vec![
            "host-image-compaction-requires-runtime-native-tool".into(),
            "disk-sage-will-not-rewrite-vm-image".into(),
        ],
        observed_at_ms,
        plan_fingerprint: fingerprint.clone(),
        exact_approval_phrase: ready.then(|| {
            format!(
                "DiskSage {} 게스트 정리 승인 {}",
                runtime.as_str(),
                fingerprint
            )
        }),
        stop_approval_phrase: (runtime == RuntimeStorageKind::PodmanMachine
            && ready
            && running_container_count == Some(0))
        .then(|| format!("DiskSage podman-machine 비활성 정지 승인 {fingerprint}")),
        recovery_approval_phrase: recovery_ready.then(|| {
            format!(
                "DiskSage {} 연결 복구 승인 {}",
                runtime.as_str(),
                fingerprint
            )
        }),
        evidence_complete: executable_available
            && guest_running.is_some()
            && (guest_running != Some(true) || guest_reachable.is_some()),
        issue,
    }
}

/// Inspect exactly one selected runtime without probing unrelated runtimes.
pub fn inspect_one(runtime: RuntimeStorageKind) -> RuntimeStoragePlan {
    inspect_runtime(runtime, now_ms())
}

/// Stop a reachable Podman machine only after a fresh native query proves zero running containers.
pub fn execute_inactive_stop(
    confirmation_phrase: &str,
    rationale: &str,
) -> Result<RuntimeStorageStopExecution, String> {
    if rationale.trim().is_empty()
        || rationale != rationale.trim()
        || rationale.chars().count() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("runtime-storage-rationale-invalid".into());
    }
    let runtime = RuntimeStorageKind::PodmanMachine;
    let plan = inspect_runtime(runtime, now_ms());
    let expected = plan
        .stop_approval_phrase
        .as_deref()
        .ok_or("runtime-storage-stop-not-ready")?;
    if confirmation_phrase != expected {
        return Err("runtime-storage-confirmation-mismatch".into());
    }
    let running_container_count_before = plan
        .running_container_count
        .ok_or("runtime-storage-container-count-unavailable")?;
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let before = home
        .as_deref()
        .ok_or_else(|| "runtime-storage-home-unavailable".to_string())
        .and_then(|path| crate::volume_pressure::snapshot_volume(path, now_ms()));
    let image_before = crate::podman_reclaim::inspect_raw_image_evidence(
        &fixed_binary(runtime),
        crate::podman_reclaim::DEFAULT_PODMAN_MACHINE,
        COMMAND_TIMEOUT,
    );
    let args = ["machine", "stop", "podman-machine-default"];
    let output = run_bounded_with_timeout(&fixed_binary(runtime), &args, RECOVERY_TIMEOUT)?;
    let live = inspect_runtime(runtime, now_ms());
    let after = home
        .as_deref()
        .ok_or_else(|| "runtime-storage-home-unavailable".to_string())
        .and_then(|path| crate::volume_pressure::snapshot_volume(path, now_ms()));
    let image_after = crate::podman_reclaim::inspect_raw_image_evidence(
        &fixed_binary(runtime),
        crate::podman_reclaim::DEFAULT_PODMAN_MACHINE,
        COMMAND_TIMEOUT,
    );
    let (volume_comparison, volume_evidence_error) = match (before, after) {
        (Ok(before), Ok(after)) => crate::volume_pressure::compare_snapshots(&before, &after, None)
            .map(|comparison| (Some(comparison), None))
            .unwrap_or_else(|error| (None, Some(error))),
        (Err(error), _) | (_, Err(error)) => (None, Some(error)),
    };
    let (image_before_bytes, image_after_bytes, image_reclaimed, image_error) =
        match (image_before, image_after) {
            (Ok(before), Ok(after)) if before.path == after.path => {
                let before = before.allocated_bytes;
                let after = after.allocated_bytes;
                (
                    before,
                    after,
                    before
                        .zip(after)
                        .map(|(before, after)| before.saturating_sub(after)),
                    None,
                )
            }
            (Ok(_), Ok(_)) => (
                None,
                None,
                None,
                Some("runtime-storage-image-changed".into()),
            ),
            (Err(error), _) | (_, Err(error)) => (None, None, None, Some(error)),
        };
    Ok(RuntimeStorageStopExecution {
        schema_kind: "disksage.runtime-storage-stop-execution",
        schema_version: SCHEMA_VERSION,
        runtime,
        command: plan.stop_command.unwrap_or_default(),
        status_code: output.0,
        stdout: output.1,
        stderr: output.2,
        output_truncated: output.3,
        running_container_count_before,
        guest_running_after: live.guest_running,
        executed: output.0 == 0 && live.guest_running == Some(false),
        executed_at_ms: now_ms(),
        rationale: rationale.into(),
        volume_comparison,
        volume_evidence_error,
        runtime_image_allocated_bytes_before: image_before_bytes,
        runtime_image_allocated_bytes_after: image_after_bytes,
        runtime_image_reclaimed_bytes: image_reclaimed,
        runtime_image_evidence_error: image_error,
    })
}

/// Restart a runtime only when it reports running but its guest is unreachable.
pub fn execute_recovery(
    runtime: RuntimeStorageKind,
    confirmation_phrase: &str,
    rationale: &str,
) -> Result<RuntimeStorageRecoveryExecution, String> {
    if rationale.trim().is_empty()
        || rationale != rationale.trim()
        || rationale.chars().count() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("runtime-storage-rationale-invalid".into());
    }
    let plan = inspect_runtime(runtime, now_ms());
    let expected = plan
        .recovery_approval_phrase
        .as_deref()
        .ok_or("runtime-storage-recovery-not-ready")?;
    if confirmation_phrase != expected {
        return Err("runtime-storage-confirmation-mismatch".into());
    }
    let binary = fixed_binary(runtime);
    let (stop_args, start_args): (&[&str], &[&str]) = match runtime {
        RuntimeStorageKind::PodmanMachine => (
            &["machine", "stop", "podman-machine-default"],
            &["machine", "start", "podman-machine-default"],
        ),
        RuntimeStorageKind::Colima => (&["stop"], &["start"]),
    };
    let stop = run_bounded_with_timeout(&binary, stop_args, RECOVERY_TIMEOUT)?;
    if stop.0 != 0 {
        return Err("runtime-storage-recovery-stop-failed".into());
    }
    let start = run_bounded_with_timeout(&binary, start_args, RECOVERY_TIMEOUT)?;
    if start.0 != 0 {
        return Err("runtime-storage-recovery-start-failed".into());
    }
    let live = inspect_runtime(runtime, now_ms());
    let reachable = live.guest_reachable == Some(true);
    Ok(RuntimeStorageRecoveryExecution {
        schema_kind: "disksage.runtime-storage-recovery-execution",
        schema_version: SCHEMA_VERSION,
        runtime,
        command: plan.recovery_command.unwrap_or_default(),
        stop_status_code: stop.0,
        start_status_code: start.0,
        guest_reachable_after_recovery: reachable,
        executed: reachable,
        executed_at_ms: now_ms(),
        rationale: rationale.into(),
    })
}

/// Inspect both supported VM-backed runtimes without mutating their stores.
pub fn inspect() -> Vec<RuntimeStoragePlan> {
    let observed_at_ms = now_ms();
    [
        RuntimeStorageKind::PodmanMachine,
        RuntimeStorageKind::Colima,
    ]
    .into_iter()
    .map(|runtime| inspect_runtime(runtime, observed_at_ms))
    .collect()
}

/// Run guest `fstrim` only after the exact, fresh plan phrase has been approved.
pub fn execute_trim(
    runtime: RuntimeStorageKind,
    confirmation_phrase: &str,
    rationale: &str,
) -> Result<RuntimeStorageExecution, String> {
    if rationale.trim().is_empty()
        || rationale != rationale.trim()
        || rationale.chars().count() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("runtime-storage-rationale-invalid".into());
    }
    let observed_at_ms = now_ms();
    let plan = inspect_runtime(runtime, observed_at_ms);
    let expected = plan
        .exact_approval_phrase
        .as_deref()
        .ok_or("runtime-storage-trim-not-ready")?;
    if confirmation_phrase != expected {
        return Err("runtime-storage-confirmation-mismatch".into());
    }
    let args = match runtime {
        RuntimeStorageKind::PodmanMachine => [
            "machine",
            "ssh",
            "podman-machine-default",
            "--",
            "sudo",
            "fstrim",
            "-av",
        ]
        .as_slice(),
        RuntimeStorageKind::Colima => ["ssh", "--", "sudo", "fstrim", "-av"].as_slice(),
    };
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let before = home
        .as_deref()
        .ok_or_else(|| "runtime-storage-home-unavailable".to_string())
        .and_then(|path| crate::volume_pressure::snapshot_volume(path, now_ms()));
    let image_before = (runtime == RuntimeStorageKind::PodmanMachine).then(|| {
        crate::podman_reclaim::inspect_raw_image_evidence(
            &fixed_binary(runtime),
            crate::podman_reclaim::DEFAULT_PODMAN_MACHINE,
            COMMAND_TIMEOUT,
        )
    });
    let output = run_bounded_with_timeout(&fixed_binary(runtime), args, RECOVERY_TIMEOUT)?;
    let after = home
        .as_deref()
        .ok_or_else(|| "runtime-storage-home-unavailable".to_string())
        .and_then(|path| crate::volume_pressure::snapshot_volume(path, now_ms()));
    let image_after = (runtime == RuntimeStorageKind::PodmanMachine).then(|| {
        crate::podman_reclaim::inspect_raw_image_evidence(
            &fixed_binary(runtime),
            crate::podman_reclaim::DEFAULT_PODMAN_MACHINE,
            COMMAND_TIMEOUT,
        )
    });
    let (volume_comparison, volume_evidence_error) = match (before, after) {
        (Ok(before), Ok(after)) => {
            match crate::volume_pressure::compare_snapshots(&before, &after, None) {
                Ok(comparison) => (Some(comparison), None),
                Err(error) => (None, Some(error)),
            }
        }
        (Err(error), _) | (_, Err(error)) => (None, Some(error)),
    };
    let (
        runtime_image_allocated_bytes_before,
        runtime_image_allocated_bytes_after,
        runtime_image_reclaimed_bytes,
        runtime_image_evidence_error,
    ) = match (image_before, image_after) {
        (Some(Ok(before)), Some(Ok(after))) if before.path == after.path => {
            let before = before.allocated_bytes;
            let after = after.allocated_bytes;
            (
                before,
                after,
                before
                    .zip(after)
                    .map(|(before, after)| before.saturating_sub(after)),
                None,
            )
        }
        (Some(Ok(_)), Some(Ok(_))) => (
            None,
            None,
            None,
            Some("runtime-storage-image-changed".into()),
        ),
        (Some(Err(error)), _) | (_, Some(Err(error))) => (None, None, None, Some(error)),
        (None, None) => (None, None, None, None),
        _ => (
            None,
            None,
            None,
            Some("runtime-storage-image-evidence-incomplete".into()),
        ),
    };
    Ok(RuntimeStorageExecution {
        schema_kind: "disksage.runtime-storage-execution",
        schema_version: SCHEMA_VERSION,
        runtime,
        command: trim_command(runtime),
        status_code: output.0,
        stdout: output.1,
        stderr: output.2,
        output_truncated: output.3,
        executed: output.0 == 0,
        executed_at_ms: now_ms(),
        rationale: rationale.into(),
        volume_comparison,
        volume_evidence_error,
        runtime_image_allocated_bytes_before,
        runtime_image_allocated_bytes_after,
        runtime_image_reclaimed_bytes,
        runtime_image_evidence_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn trim_commands_are_fixed_and_do_not_include_user_input() {
        assert_eq!(
            trim_command(RuntimeStorageKind::Colima),
            vec!["colima", "ssh", "--", "sudo", "fstrim", "-av"]
        );
        assert!(trim_command(RuntimeStorageKind::PodmanMachine)
            .contains(&"podman-machine-default".into()));
    }

    #[test]
    fn unavailable_runtime_plan_is_fail_closed() {
        let plan = inspect_runtime(RuntimeStorageKind::Colima, 42);
        assert!(!plan.host_compaction_supported);
        assert!(plan.exact_approval_phrase.is_none() || plan.guest_running == Some(true));
    }

    #[test]
    fn reachability_is_bound_into_the_plan_fingerprint() {
        assert_ne!(
            fingerprint(
                RuntimeStorageKind::PodmanMachine,
                Some(true),
                Some(true),
                Some(0)
            ),
            fingerprint(
                RuntimeStorageKind::PodmanMachine,
                Some(true),
                Some(false),
                Some(0)
            )
        );
    }

    #[test]
    fn running_container_count_changes_stop_authority() {
        assert_ne!(
            fingerprint(
                RuntimeStorageKind::PodmanMachine,
                Some(true),
                Some(true),
                Some(0)
            ),
            fingerprint(
                RuntimeStorageKind::PodmanMachine,
                Some(true),
                Some(true),
                Some(1)
            )
        );
    }

    #[test]
    fn failed_reachability_probe_remains_incomplete() {
        assert_eq!(
            reachability_from_probe(Err("runtime-storage-command-timeout".into())),
            None
        );
        assert_eq!(
            reachability_from_probe(Ok((255, String::new(), String::new(), false))),
            Some(false)
        );
        assert_eq!(
            reachability_from_probe(Ok((0, String::new(), String::new(), false))),
            Some(true)
        );
    }

    #[test]
    fn bounded_reader_drains_large_output_without_retaining_it() {
        let input = vec![b'x'; MAX_CAPTURE_BYTES + 1];
        let (captured, truncated) = drain_bounded(Cursor::new(input)).expect("reader succeeds");
        assert_eq!(captured.len(), MAX_CAPTURE_BYTES);
        assert!(truncated);
    }

    #[test]
    fn trim_rejects_control_characters_before_runtime_probe() {
        assert_eq!(
            execute_trim(RuntimeStorageKind::Colima, "", "operator\u{0007}note").unwrap_err(),
            "runtime-storage-rationale-invalid"
        );
    }
}
