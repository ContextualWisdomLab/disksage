//! Read-only planning and explicit guest trim for Podman and Colima storage.
//!
//! A VM-backed runtime can report a large logical store while its sparse host image keeps
//! allocated extents. DiskSage therefore separates guest `fstrim` from host-image compaction:
//! trim is an optional, bounded command; raw-image compaction is never guessed or run by the app.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const SCHEMA_VERSION: u32 = 1;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
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
    pub trim_command: Option<Vec<String>>,
    pub host_compaction_supported: bool,
    pub host_compaction_blockers: Vec<String>,
    pub observed_at_ms: u64,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: Option<String>,
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
    pub stdout: String,
    pub stderr: String,
    pub output_truncated: bool,
    pub executed: bool,
    pub executed_at_ms: u64,
    pub rationale: String,
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

fn run_bounded(binary: &Path, args: &[&str]) -> Result<(i32, String, String, bool), String> {
    let binary = binary.to_path_buf();
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = Command::new(binary)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map(|output| {
                let (stdout, stdout_truncated) = bounded_output(output.stdout);
                let (stderr, stderr_truncated) = bounded_output(output.stderr);
                (
                    output.status.code().unwrap_or(-1),
                    stdout,
                    stderr,
                    stdout_truncated || stderr_truncated,
                )
            })
            .map_err(|_| "runtime-storage-command-failed".to_string());
        let _ = sender.send(result);
    });
    receiver
        .recv_timeout(COMMAND_TIMEOUT)
        .map_err(|_| "runtime-storage-command-timeout".to_string())?
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

fn fingerprint(runtime: RuntimeStorageKind, running: Option<bool>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.runtime-storage-plan.v1\0");
    hasher.update(runtime.as_str().as_bytes());
    hasher.update([running.unwrap_or(false) as u8]);
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
                let running = match runtime {
                    RuntimeStorageKind::PodmanMachine => {
                        stdout.trim().eq_ignore_ascii_case("running")
                    }
                    RuntimeStorageKind::Colima => {
                        serde_json::from_str::<serde_json::Value>(&stdout)
                            .ok()
                            .and_then(|value| {
                                value.get("status").and_then(serde_json::Value::as_str)
                            })
                            .is_some_and(|status| status.eq_ignore_ascii_case("running"))
                    }
                };
                if runtime == RuntimeStorageKind::Colima
                    && serde_json::from_str::<serde_json::Value>(&stdout)
                        .ok()
                        .and_then(|value| value.get("status").and_then(serde_json::Value::as_str))
                        .is_none()
                {
                    (None, Some("runtime-storage-state-invalid".into()))
                } else {
                    (Some(running), None)
                }
            }
            Ok(_) => (None, Some("runtime-storage-state-unavailable".into())),
            Err(error) => (None, Some(error)),
        }
    };
    let fingerprint = fingerprint(runtime, guest_running);
    let ready = executable_available && guest_running == Some(true);
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
        trim_command: ready.then(|| trim_command(runtime)),
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
        evidence_complete: executable_available && guest_running.is_some(),
        issue,
    }
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
    let output = run_bounded(&fixed_binary(runtime), args)?;
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
