//! Read-only evidence for files that are unlinked but still held open by a process.
//!
//! These files no longer have a pathname to clean. DiskSage never terminates the holder; it only
//! reports bounded local evidence so the person can close the owning app normally and rescan.

use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

const MAX_CAPTURE_BYTES: usize = 4 * 1024 * 1024;
const MAX_RECORDS: usize = 10_000;

fn lsof_result_is_usable(
    status_success: bool,
    status_code: Option<i32>,
    output: &[u8],
    stderr: &[u8],
) -> bool {
    status_success || (status_code == Some(1) && output.is_empty() && stderr.is_empty())
}

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeletedOpenActionItem {
    pub application: String,
    pub holder_count: u64,
    pub distinct_file_count: u64,
    pub observed_logical_bytes: u64,
    pub customer_next_action: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeletedOpenActionReceipt {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub receipt_id: String,
    pub observed_at_ms: u64,
    pub evidence_complete: bool,
    pub observed_file_count: u64,
    pub observed_logical_bytes: u64,
    pub physically_reclaimable_bytes: Option<u64>,
    pub mutation_executed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeletedOpenActionPlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub evidence_complete: bool,
    pub observed_file_count: u64,
    pub observed_logical_bytes: u64,
    pub physically_reclaimable_bytes: Option<u64>,
    pub actions: Vec<DeletedOpenActionItem>,
    pub receipt: DeletedOpenActionReceipt,
    pub customer_next_action: &'static str,
    pub local_paths_included: bool,
    pub process_termination_executed: bool,
    pub mutation_executed: bool,
}

#[derive(Default)]
struct ProcessAccumulator {
    command: String,
    file_count: u64,
    logical_bytes: u64,
}

/// Converts a path-free audit into a customer action plan and immutable-value read receipt.
pub fn plan_from_audit(
    audit: DeletedOpenAuditReport,
    observed_at_ms: u64,
) -> DeletedOpenActionPlan {
    let commands_complete = audit
        .processes
        .iter()
        .all(|process| !process.command.trim().is_empty());
    let evidence_complete = audit.evidence_complete && commands_complete;
    let mut grouped: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    for process in &audit.processes {
        if process.command.trim().is_empty() {
            continue;
        }
        let entry = grouped.entry(process.command.clone()).or_default();
        entry.0 = entry.0.saturating_add(1);
        entry.1 = entry.1.saturating_add(process.distinct_file_count);
        entry.2 = entry.2.saturating_add(process.observed_logical_bytes);
    }
    let mut actions = grouped
        .into_iter()
        .map(
            |(application, (holder_count, distinct_file_count, observed_logical_bytes))| {
                DeletedOpenActionItem {
                    customer_next_action: format!(
                        "Quit every {application} window normally, then scan again."
                    ),
                    application,
                    holder_count,
                    distinct_file_count,
                    observed_logical_bytes,
                }
            },
        )
        .collect::<Vec<_>>();
    actions.sort_by(|left, right| {
        right
            .observed_logical_bytes
            .cmp(&left.observed_logical_bytes)
            .then_with(|| left.application.cmp(&right.application))
    });
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-deleted-open-action-receipt-v1\0");
    hasher.update(&observed_at_ms.to_le_bytes());
    hasher.update(&audit.observed_file_count.to_le_bytes());
    hasher.update(&audit.observed_logical_bytes.to_le_bytes());
    hasher.update(&[u8::from(evidence_complete)]);
    for action in &actions {
        hasher.update(action.application.as_bytes());
        hasher.update(&[0]);
        hasher.update(&action.holder_count.to_le_bytes());
        hasher.update(&action.distinct_file_count.to_le_bytes());
        hasher.update(&action.observed_logical_bytes.to_le_bytes());
    }
    let receipt = DeletedOpenActionReceipt {
        schema_kind: "disksage.deleted-open-action-receipt",
        schema_version: 1,
        receipt_id: hasher.finalize().to_hex().to_string(),
        observed_at_ms,
        evidence_complete,
        observed_file_count: audit.observed_file_count,
        observed_logical_bytes: audit.observed_logical_bytes,
        physically_reclaimable_bytes: None,
        mutation_executed: false,
    };
    DeletedOpenActionPlan {
        schema_kind: "disksage.deleted-open-action-plan",
        schema_version: 1,
        evidence_complete,
        observed_file_count: audit.observed_file_count,
        observed_logical_bytes: audit.observed_logical_bytes,
        physically_reclaimable_bytes: None,
        actions,
        receipt,
        customer_next_action: if evidence_complete {
            "Quit every listed app normally, then scan again."
        } else {
            "Keep the apps open and scan again after DiskSage can complete this check."
        },
        local_paths_included: false,
        process_termination_executed: false,
        mutation_executed: false,
    }
}

/// Parses bounded `lsof +L1 -F0pcfDist` output without retaining deleted pathnames.
pub fn parse_lsof_nul(output: &[u8], capture_truncated: bool) -> DeletedOpenAuditReport {
    let mut process_id = None;
    let mut command = String::new();
    let mut device = None;
    let mut inode = None;
    let mut size = None;
    let mut file_type = None;
    let mut file_started = false;
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
                           file_type: &mut Option<String>,
                           file_started: &mut bool| {
        if !*file_started {
            return;
        }
        *file_started = false;
        record_count += 1;
        if record_count > MAX_RECORDS {
            evidence_complete = false;
        } else if file_type.is_none() {
            evidence_complete = false;
        } else if file_type.as_deref() != Some("REG") {
            // Non-regular descriptors do not represent held filesystem file capacity.
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
                    &mut file_started,
                );
                process_id = std::str::from_utf8(value).ok().and_then(|v| v.parse().ok());
                command.clear();
            }
            b'c' => command = String::from_utf8_lossy(value).into_owned(),
            b'f' => {
                finish_file(
                    process_id,
                    &command,
                    &mut device,
                    &mut inode,
                    &mut size,
                    &mut file_type,
                    &mut file_started,
                );
                file_started = true;
            }
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
        &mut file_started,
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
        .stderr(Stdio::piped());
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
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "deleted-open-lsof-error-output-unavailable".to_string())?;
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
    let error_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr
            .take(64 * 1024)
            .read_to_end(&mut bytes)
            .map_err(|_| "deleted-open-lsof-error-read-failed".to_string())?;
        Ok::<_, String>(bytes)
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
                let _ = error_reader.join();
                return Err("deleted-open-lsof-timeout".into());
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => return Err("deleted-open-lsof-wait-failed".into()),
        }
    };
    let (bytes, truncated) = reader
        .join()
        .map_err(|_| "deleted-open-lsof-reader-failed".to_string())??;
    let error_bytes = error_reader
        .join()
        .map_err(|_| "deleted-open-lsof-error-reader-failed".to_string())??;
    if !lsof_result_is_usable(status.success(), status.code(), &bytes, &error_bytes) {
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
            (&b"p10\0cEditor\0f1\0"[..], false),
            (&b"p10\0cEditor\0f1\0D1\0i20\0s4096\0"[..], false),
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
    fn lsof_documented_empty_result_is_usable_but_errors_are_not() {
        assert!(lsof_result_is_usable(false, Some(1), b"", b""));
        assert!(!lsof_result_is_usable(
            false,
            Some(1),
            b"",
            b"permission denied"
        ));
        assert!(!lsof_result_is_usable(false, Some(2), b"", b""));
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
    fn non_regular_descriptors_still_obey_record_bound() {
        let mut output = b"p10\0cWorker\0".to_vec();
        for descriptor in 0..=MAX_RECORDS {
            output.extend_from_slice(format!("f{descriptor}\0tPIPE\0").as_bytes());
        }
        let report = parse_lsof_nul(&output, false);
        assert!(!report.evidence_complete);
        assert_eq!(report.observed_file_count, 0);
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

    #[test]
    fn action_plan_groups_apps_without_granting_termination_or_reclaim_credit() {
        let audit = parse_lsof_nul(
            b"p10\0cEditor\0f1\0tREG\0D1\0i20\0s4096\0p11\0cEditor\0f2\0tREG\0D1\0i21\0s512\0",
            false,
        );
        let plan = plan_from_audit(audit, 123);
        assert!(plan.evidence_complete);
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].application, "Editor");
        assert_eq!(plan.actions[0].holder_count, 2);
        assert_eq!(plan.observed_logical_bytes, 4_608);
        assert_eq!(plan.physically_reclaimable_bytes, None);
        assert!(!plan.process_termination_executed);
        assert!(!plan.mutation_executed);
        assert!(!serde_json::to_string(&plan).unwrap().contains("/private/"));
    }

    #[test]
    fn action_plan_with_missing_app_identity_is_incomplete_and_has_no_action() {
        let audit = parse_lsof_nul(b"p10\0c\0f1\0tREG\0D1\0i20\0s4096\0", false);
        let plan = plan_from_audit(audit, 123);
        assert!(!plan.evidence_complete);
        assert!(plan.actions.is_empty());
        assert_eq!(
            plan.customer_next_action,
            "Keep the apps open and scan again after DiskSage can complete this check."
        );
    }

    #[test]
    fn action_plan_orders_largest_customer_action_first() {
        let audit = parse_lsof_nul(
            b"p10\0cSmall\0f1\0tREG\0D1\0i20\0s10\0p11\0cLarge\0f2\0tREG\0D1\0i21\0s100\0",
            false,
        );
        let plan = plan_from_audit(audit, 123);
        assert_eq!(plan.actions[0].application, "Large");
        assert_eq!(plan.actions[1].application, "Small");
    }
}
