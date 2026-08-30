//! Read-only evidence for files that are unlinked but still held open by a process.
//!
//! These files no longer have a pathname to clean. DiskSage never terminates the holder; it only
//! reports bounded local evidence so the person can close the owning app normally and rescan.

use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

const MAX_CAPTURE_BYTES: usize = 4 * 1024 * 1024;
const MAX_RECORDS: usize = 10_000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeletedOpenProcessEvidence {
    pub process_id: u32,
    pub command: String,
    pub distinct_file_count: u64,
    pub observed_logical_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeletedOpenAuditReport {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub evidence_complete: bool,
    pub observed_file_count: u64,
    pub observed_logical_bytes: u64,
    pub physically_reclaimable_bytes: Option<u64>,
    pub processes: Vec<DeletedOpenProcessEvidence>,
    pub customer_next_action: &'static str,
    pub reason_codes: Vec<String>,
    pub local_paths_included: bool,
    pub mutation_executed: bool,
}

#[derive(Default)]
struct ProcessAccumulator {
    command: String,
    file_count: u64,
    logical_bytes: u64,
}

/// Parses bounded `lsof +L1 -F0pcfDist` output without retaining deleted pathnames.
pub fn parse_lsof_nul(output: &[u8], capture_truncated: bool) -> DeletedOpenAuditReport {
    let mut process_id = None;
    let mut command = String::new();
    let mut device = None;
    let mut inode = None;
    let mut size = None;
    let mut file_type = None;
    let mut seen_files = HashSet::new();
    let mut seen_holders = HashSet::new();
    let mut observed_file_count = 0u64;
    let mut observed_logical_bytes = 0u64;
    let mut processes: BTreeMap<u32, ProcessAccumulator> = BTreeMap::new();
    let mut evidence_complete = !capture_truncated;
    let mut record_count = 0usize;

    let mut finish_file = |process_id: Option<u32>,
                           command: &str,
                           device: &mut Option<String>,
                           inode: &mut Option<u64>,
                           size: &mut Option<u64>,
                           file_type: &mut Option<String>| {
        if device.is_none() && inode.is_none() && size.is_none() && file_type.is_none() {
            return;
        }
        record_count += 1;
        if file_type.as_deref() != Some("REG") {
            // Non-regular descriptors do not represent held filesystem file capacity.
        } else if record_count > MAX_RECORDS {
            evidence_complete = false;
        } else if let (Some(pid), Some(device), Some(inode), Some(bytes)) =
            (process_id, device.take(), inode.take(), size.take())
        {
            let identity = (device, inode);
            if seen_files.insert(identity.clone()) {
                observed_file_count = observed_file_count.saturating_add(1);
                observed_logical_bytes = observed_logical_bytes.saturating_add(bytes);
            }
            if seen_holders.insert((pid, identity)) {
                let process = processes.entry(pid).or_default();
                process.command = command
                    .chars()
                    .filter(|character| !character.is_control())
                    .take(256)
                    .collect();
                process.file_count = process.file_count.saturating_add(1);
                process.logical_bytes = process.logical_bytes.saturating_add(bytes);
            }
        } else {
            evidence_complete = false;
        }
        *device = None;
        *inode = None;
        *size = None;
        *file_type = None;
    };

    for raw in output.split(|byte| *byte == 0) {
        let raw = raw.strip_prefix(b"\n").unwrap_or(raw);
        let Some((&field, value)) = raw.split_first() else {
            continue;
        };
        match field {
            b'p' => {
                finish_file(
                    process_id,
                    &command,
                    &mut device,
                    &mut inode,
                    &mut size,
                    &mut file_type,
                );
                process_id = std::str::from_utf8(value).ok().and_then(|v| v.parse().ok());
                command.clear();
            }
            b'c' => command = String::from_utf8_lossy(value).into_owned(),
            b'f' => finish_file(
                process_id,
                &command,
                &mut device,
                &mut inode,
                &mut size,
                &mut file_type,
            ),
            b'D' => device = Some(String::from_utf8_lossy(value).into_owned()),
            b'i' => inode = std::str::from_utf8(value).ok().and_then(|v| v.parse().ok()),
            b's' => size = std::str::from_utf8(value).ok().and_then(|v| v.parse().ok()),
            b't' => file_type = Some(String::from_utf8_lossy(value).into_owned()),
            b'n' => {}
            _ => {}
        }
    }
    finish_file(
        process_id,
        &command,
        &mut device,
        &mut inode,
        &mut size,
        &mut file_type,
    );

    let processes = processes
        .into_iter()
        .map(|(process_id, value)| DeletedOpenProcessEvidence {
            process_id,
            command: value.command,
            distinct_file_count: value.file_count,
            observed_logical_bytes: value.logical_bytes,
        })
        .collect::<Vec<_>>();
    let mut reason_codes = vec!["deleted-open-size-is-not-physical-reclaim-proof".into()];
    if !evidence_complete {
        reason_codes.push("deleted-open-evidence-incomplete".into());
    }
    DeletedOpenAuditReport {
        schema_kind: "disksage.deleted-open-audit",
        schema_version: 1,
        evidence_complete,
        observed_file_count,
        observed_logical_bytes,
        physically_reclaimable_bytes: None,
        processes,
        customer_next_action: "Close the listed apps normally, then scan again.",
        reason_codes,
        local_paths_included: false,
        mutation_executed: false,
    }
}

#[cfg(unix)]
pub fn collect_deleted_open_audit() -> Result<DeletedOpenAuditReport, String> {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    let executable = if cfg!(target_os = "macos") {
        "/usr/sbin/lsof"
    } else {
        "/usr/bin/lsof"
    };
    let mut command = Command::new(executable);
    command
        .args(["-nP", "-w", "+L1", "-F0pcfDist"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
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
        .map_err(|_| "deleted-open-lsof-unavailable".to_string())?;
    let pid = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "deleted-open-lsof-output-unavailable".to_string())?;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::with_capacity(64 * 1024);
        let mut truncated = false;
        for byte in stdout.bytes() {
            let byte = byte.map_err(|_| "deleted-open-lsof-read-failed".to_string())?;
            if bytes.len() < MAX_CAPTURE_BYTES {
                bytes.push(byte);
            } else {
                truncated = true;
            }
        }
        Ok::<_, String>((bytes, truncated))
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                unsafe {
                    let _ = libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
                }
                let _ = child.wait();
                let _ = reader.join();
                return Err("deleted-open-lsof-timeout".into());
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => return Err("deleted-open-lsof-wait-failed".into()),
        }
    };
    let (bytes, truncated) = reader
        .join()
        .map_err(|_| "deleted-open-lsof-reader-failed".to_string())??;
    if !status.success() {
        return Err("deleted-open-lsof-failed".into());
    }
    Ok(parse_lsof_nul(&bytes, truncated))
}

#[cfg(not(unix))]
pub fn collect_deleted_open_audit() -> Result<DeletedOpenAuditReport, String> {
    Err("deleted-open-audit-unsupported-platform".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_deduplicates_file_identity_and_never_retains_paths() {
        let output = b"p10\0cEditor\0f1\0tREG\0D1\0i20\0s4096\0n/private/a (deleted)\0f2\0tREG\0D1\0i20\0s4096\0n/private/a (deleted)\0p11\0cWorker\0f3\0tREG\0D1\0i21\0s512\0n/private/b (deleted)\0";
        let report = parse_lsof_nul(output, false);
        assert!(report.evidence_complete);
        assert_eq!(report.observed_file_count, 2);
        assert_eq!(report.observed_logical_bytes, 4_608);
        assert_eq!(report.physically_reclaimable_bytes, None);
        assert_eq!(report.processes.len(), 2);
        let encoded = serde_json::to_string(&report).unwrap();
        assert!(!encoded.contains("/private/"));
        assert!(!encoded.contains("(deleted)"));
        assert!(!report.mutation_executed);
    }

    #[test]
    fn incomplete_identity_or_truncation_fails_closed() {
        for (output, truncated) in [
            (
                &b"p10\0cEditor\0f1\0tREG\0s4096\0n/private/a (deleted)\0"[..],
                false,
            ),
            (&b"p10\0cEditor\0f1\0tREG\0D1\0i20\0s4096\0"[..], true),
        ] {
            let report = parse_lsof_nul(output, truncated);
            assert!(!report.evidence_complete);
            assert_eq!(report.physically_reclaimable_bytes, None);
            assert!(report
                .reason_codes
                .contains(&"deleted-open-evidence-incomplete".into()));
        }
    }

    #[test]
    fn non_regular_descriptors_are_ignored_without_becoming_capacity() {
        let report = parse_lsof_nul(b"p10\0cWorker\0f1\0tPIPE\0D1\0i20\0s4096\0", false);
        assert!(report.evidence_complete);
        assert_eq!(report.observed_file_count, 0);
        assert_eq!(report.observed_logical_bytes, 0);
        assert!(report.processes.is_empty());
    }

    #[test]
    fn shared_file_counts_once_but_lists_every_holder() {
        let output =
            b"p10\0cEditor\0f1\0tREG\0D1\0i20\0s4096\0p11\0cWorker\0f2\0tREG\0D1\0i20\0s4096\0";
        let report = parse_lsof_nul(output, false);
        assert_eq!(report.observed_file_count, 1);
        assert_eq!(report.observed_logical_bytes, 4_096);
        assert_eq!(report.processes.len(), 2);
        assert!(report
            .processes
            .iter()
            .all(|process| process.distinct_file_count == 1));
    }
}
