//! Lossless, transparent filesystem compression for inactive structured logs on macOS.

use crate::git_worktree::run_bounded_command;
use serde::Serialize;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

const MAX_FILES: usize = 10_000;
const MIN_BYTES: u64 = 100 * 1024 * 1024;
const COMPRESSION_CONCURRENCY: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransparentCompressionCandidate {
    pub path: String,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub modified_ms: u64,
    pub device: u64,
    pub inode: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransparentCompressionPlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub root: String,
    pub minimum_age_days: u64,
    pub max_files: usize,
    pub compression_concurrency: usize,
    pub candidate_count: usize,
    pub logical_bytes: u64,
    pub allocated_bytes_before: u64,
    pub candidates: Vec<TransparentCompressionCandidate>,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: Option<String>,
    pub filesystem_mutation_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransparentCompressionResult {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub plan_fingerprint: String,
    pub compressed_count: usize,
    pub not_compressible_count: usize,
    pub failed_count: usize,
    pub stopped_reason: Option<String>,
    pub failures: Vec<String>,
    pub allocated_bytes_before: u64,
    pub allocated_bytes_after: u64,
    pub candidate_allocated_bytes_reduction: u64,
    pub host_available_bytes_delta_during_execution: i64,
    pub physically_reclaimed_bytes: Option<u64>,
    pub rationale: String,
    pub filesystem_mutation_executed: bool,
    pub content_identity_verified: bool,
    pub verification_complete: bool,
    pub recoverability: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompressionProgress {
    compressed_count: usize,
    not_compressible_count: usize,
    failed_count: usize,
    allocated_bytes_after_upper_bound: u64,
    stopped_reason: Option<String>,
    failures: Vec<String>,
}

#[cfg(target_os = "macos")]
fn is_transparently_compressed(metadata: &fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    metadata.st_flags() & 0x20 != 0
}

#[cfg(not(target_os = "macos"))]
fn is_transparently_compressed(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn observation(path: &Path) -> Result<TransparentCompressionCandidate, String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path).map_err(|_| "compression-metadata-unavailable")?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err("compression-file-unsafe".into());
    }
    Ok(TransparentCompressionCandidate {
        path: path.to_string_lossy().into_owned(),
        logical_bytes: metadata.len(),
        allocated_bytes: metadata.blocks().saturating_mul(512),
        modified_ms: u64::try_from(metadata.mtime())
            .unwrap_or_default()
            .saturating_mul(1_000),
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(not(unix))]
fn observation(_path: &Path) -> Result<TransparentCompressionCandidate, String> {
    Err("transparent-compression-unsupported-platform".into())
}

fn fingerprint(
    root: &Path,
    minimum_age_days: u64,
    max_files: usize,
    candidates: &[TransparentCompressionCandidate],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.transparent-compression.v1\0");
    hasher.update(root.as_os_str().to_string_lossy().as_bytes());
    hasher.update(&minimum_age_days.to_le_bytes());
    hasher.update(&(max_files as u64).to_le_bytes());
    for candidate in candidates {
        hasher.update(candidate.path.as_bytes());
        for value in [
            candidate.logical_bytes,
            candidate.allocated_bytes,
            candidate.modified_ms,
            candidate.device,
            candidate.inode,
        ] {
            hasher.update(&value.to_le_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

pub fn plan(
    root: &Path,
    minimum_age_days: u64,
    max_files: usize,
    now_ms: u64,
) -> Result<TransparentCompressionPlan, String> {
    if !cfg!(target_os = "macos")
        || !root.is_absolute()
        || minimum_age_days == 0
        || max_files == 0
        || max_files > MAX_FILES
        || now_ms == 0
    {
        return Err("transparent-compression-options-invalid".into());
    }
    let root = fs::canonicalize(root).map_err(|_| "compression-root-unavailable")?;
    let cutoff = now_ms.saturating_sub(minimum_age_days.saturating_mul(86_400_000));
    let mut pending = vec![root.clone()];
    let mut candidates = Vec::new();
    while let Some(path) = pending.pop() {
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| "compression-traversal-incomplete")?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            let mut children = fs::read_dir(&path)
                .map_err(|_| "compression-traversal-incomplete")?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "compression-traversal-incomplete")?;
            children.sort_by_key(|entry| entry.file_name());
            pending.extend(children.into_iter().rev().map(|entry| entry.path()));
            continue;
        }
        if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
        {
            if is_transparently_compressed(&metadata) {
                continue;
            }
            let candidate = observation(&path)?;
            if candidate.logical_bytes >= MIN_BYTES && candidate.modified_ms <= cutoff {
                candidates.push(candidate);
                if candidates.len() > max_files {
                    return Err("transparent-compression-file-limit-exceeded".into());
                }
            }
        }
    }
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    let plan_fingerprint = fingerprint(&root, minimum_age_days, max_files, &candidates);
    Ok(TransparentCompressionPlan {
        schema_kind: "disksage.transparent-compression-plan",
        schema_version: 1,
        ontology_class: "https://disksage.app/ontology#StructuredLogArtifact",
        root: root.to_string_lossy().into_owned(),
        minimum_age_days,
        max_files,
        compression_concurrency: COMPRESSION_CONCURRENCY,
        candidate_count: candidates.len(),
        logical_bytes: candidates
            .iter()
            .map(|candidate| candidate.logical_bytes)
            .sum(),
        allocated_bytes_before: candidates
            .iter()
            .map(|candidate| candidate.allocated_bytes)
            .sum(),
        exact_approval_phrase: (!candidates.is_empty()).then(|| {
            format!("DiskSage transparent compression 승인 {plan_fingerprint}")
        }),
        candidates,
        plan_fingerprint,
        filesystem_mutation_executed: false,
    })
}

fn digest(path: &Path) -> Result<blake3::Hash, String> {
    let file = File::open(path).map_err(|_| "compression-content-read-failed")?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|_| "compression-content-read-failed")?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize())
}

#[cfg(unix)]
fn available_bytes(path: &Path) -> Result<u64, String> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| "compression-filesystem-path-invalid")?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) } != 0 {
        return Err("compression-filesystem-capacity-unavailable".into());
    }
    let stats = unsafe { stats.assume_init() };
    Ok(u64::from(stats.f_bavail).saturating_mul(stats.f_frsize))
}

#[cfg(not(unix))]
fn available_bytes(_path: &Path) -> Result<u64, String> {
    Err("transparent-compression-unsupported-platform".into())
}

fn activity_probe_proves_inactive(
    status_code: Option<i32>,
    timed_out: bool,
    stdout_truncated: bool,
    stderr_truncated: bool,
) -> bool {
    !timed_out && !stdout_truncated && !stderr_truncated && status_code == Some(1)
}

#[cfg(target_os = "macos")]
fn compress_one(candidate: &TransparentCompressionCandidate) -> Result<(u64, bool), String> {
    let path = Path::new(&candidate.path);
    if observation(path)? != *candidate {
        return Err("compression-file-changed-after-plan".into());
    }
    let lsof = run_bounded_command(
        "/usr/sbin/lsof",
        &[OsString::from("--"), path.as_os_str().to_owned()],
        Path::new("/"),
        15_000,
    )?;
    if !activity_probe_proves_inactive(
        lsof.status_code,
        lsof.timed_out,
        lsof.stdout_truncated,
        lsof.stderr_truncated,
    ) {
        return Err("compression-file-active-or-unresolved".into());
    }
    let before = digest(path)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("compression-filename-invalid")?;
    let temporary =
        path.with_file_name(format!(".{name}.disksage-compress-{}", std::process::id()));
    if temporary.exists() {
        return Err("compression-temporary-path-exists".into());
    }
    let output = run_bounded_command(
        "/usr/bin/ditto",
        &[
            OsString::from("--hfsCompression"),
            path.as_os_str().to_owned(),
            temporary.as_os_str().to_owned(),
        ],
        Path::new("/"),
        600_000,
    )?;
    if output.timed_out
        || output.stdout_truncated
        || output.stderr_truncated
        || output.status_code != Some(0)
    {
        let _ = fs::remove_file(&temporary);
        return Err("transparent-compression-command-failed".into());
    }
    let result = (|| {
        if digest(&temporary)? != before
            || fs::metadata(&temporary)
                .map_err(|_| "compression-output-unavailable")?
                .len()
                != candidate.logical_bytes
        {
            return Err("compression-content-identity-mismatch".into());
        }
        if !is_transparently_compressed(
            &fs::metadata(&temporary).map_err(|_| "compression-output-unavailable")?,
        ) {
            fs::remove_file(&temporary).map_err(|_| "compression-temporary-remove-failed")?;
            return Ok((candidate.allocated_bytes, false));
        }
        File::open(&temporary)
            .and_then(|file| file.sync_all())
            .map_err(|_| "compression-output-sync-failed")?;
        if observation(path)? != *candidate {
            return Err("compression-file-changed-before-commit".into());
        }
        fs::rename(&temporary, path).map_err(|_| "compression-atomic-replace-failed")?;
        File::open(path.parent().ok_or("compression-parent-unavailable")?)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "compression-parent-sync-failed")?;
        let after = observation(path)?.allocated_bytes;
        if digest(path)? != before {
            return Err("compression-post-commit-identity-mismatch".into());
        }
        Ok((after, true))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(not(target_os = "macos"))]
fn compress_one(_candidate: &TransparentCompressionCandidate) -> Result<(u64, bool), String> {
    Err("transparent-compression-unsupported-platform".into())
}

fn compress_candidates(
    candidates: &[TransparentCompressionCandidate],
) -> Vec<Result<(u64, bool), String>> {
    let pending = Arc::new(Mutex::new(
        candidates.iter().cloned().enumerate().collect::<Vec<_>>(),
    ));
    let results = Arc::new(Mutex::new(Vec::with_capacity(candidates.len())));
    thread::scope(|scope| {
        for _ in 0..COMPRESSION_CONCURRENCY.min(candidates.len().max(1)) {
            let pending = Arc::clone(&pending);
            let results = Arc::clone(&results);
            scope.spawn(move || loop {
                let Some((index, candidate)) = pending.lock().expect("queue poisoned").pop() else {
                    break;
                };
                results
                    .lock()
                    .expect("results poisoned")
                    .push((index, compress_one(&candidate)));
            });
        }
    });
    let results = match Arc::try_unwrap(results) {
        Ok(results) => results,
        Err(_) => unreachable!("compression workers still referenced"),
    };
    let mut results = results.into_inner().expect("results poisoned");
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

fn summarize_compression_results(
    candidates: &[TransparentCompressionCandidate],
    results: Vec<Result<(u64, bool), String>>,
) -> CompressionProgress {
    let mut progress = CompressionProgress {
        compressed_count: 0,
        not_compressible_count: 0,
        failed_count: 0,
        allocated_bytes_after_upper_bound: 0,
        stopped_reason: None,
        failures: Vec::new(),
    };
    if results.len() != candidates.len() {
        let error = "compression-result-count-mismatch".to_string();
        progress.stopped_reason = Some(error.clone());
        progress.failures.push(error);
    }
    for (index, candidate) in candidates.iter().enumerate() {
        match results.get(index) {
            Some(Ok((allocated_bytes, compressed))) => {
                progress.allocated_bytes_after_upper_bound = progress
                    .allocated_bytes_after_upper_bound
                    .saturating_add(*allocated_bytes);
                if *compressed {
                    progress.compressed_count = progress.compressed_count.saturating_add(1);
                } else {
                    progress.not_compressible_count =
                        progress.not_compressible_count.saturating_add(1);
                }
            }
            Some(Err(error)) => {
                progress.allocated_bytes_after_upper_bound = progress
                    .allocated_bytes_after_upper_bound
                    .saturating_add(candidate.allocated_bytes);
                progress.stopped_reason.get_or_insert_with(|| error.clone());
                progress.failures.push(error.clone());
            }
            None => {
                progress.allocated_bytes_after_upper_bound = progress
                    .allocated_bytes_after_upper_bound
                    .saturating_add(candidate.allocated_bytes);
            }
        }
    }
    progress.failed_count = progress.failures.len();
    progress
}

pub fn execute(
    approved_plan: &TransparentCompressionPlan,
    expected_fingerprint: &str,
    confirmation_phrase: &str,
    rationale: &str,
    now_ms: u64,
) -> Result<TransparentCompressionResult, String> {
    if rationale.trim() != rationale
        || rationale.is_empty()
        || rationale.len() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("transparent-compression-rationale-invalid".into());
    }
    let live = plan(
        Path::new(&approved_plan.root),
        approved_plan.minimum_age_days,
        approved_plan.max_files,
        now_ms,
    )?;
    if live.plan_fingerprint != approved_plan.plan_fingerprint
        || expected_fingerprint != approved_plan.plan_fingerprint
        || approved_plan.exact_approval_phrase.as_deref() != Some(confirmation_phrase)
        || live.exact_approval_phrase != approved_plan.exact_approval_phrase
    {
        return Err("transparent-compression-approval-mismatch".into());
    }
    let available_before = available_bytes(Path::new(&live.root))?;
    let progress = summarize_compression_results(
        &live.candidates,
        compress_candidates(&live.candidates),
    );
    let available_after = available_bytes(Path::new(&live.root))?;
    let available_delta = i128::from(available_after) - i128::from(available_before);
    let host_available_bytes_delta_during_execution =
        available_delta.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
    let verification_complete = progress.failed_count == 0;
    Ok(TransparentCompressionResult {
        schema_kind: "disksage.transparent-compression-result",
        schema_version: 1,
        ontology_class: live.ontology_class,
        plan_fingerprint: live.plan_fingerprint,
        compressed_count: progress.compressed_count,
        not_compressible_count: progress.not_compressible_count,
        failed_count: progress.failed_count,
        stopped_reason: progress.stopped_reason,
        failures: progress.failures,
        allocated_bytes_before: live.allocated_bytes_before,
        allocated_bytes_after: progress.allocated_bytes_after_upper_bound,
        candidate_allocated_bytes_reduction: live
            .allocated_bytes_before
            .saturating_sub(progress.allocated_bytes_after_upper_bound),
        host_available_bytes_delta_during_execution,
        physically_reclaimed_bytes: (verification_complete
            && host_available_bytes_delta_during_execution > 0)
            .then_some(host_available_bytes_delta_during_execution as u64),
        rationale: rationale.into(),
        filesystem_mutation_executed: progress.compressed_count != 0,
        content_identity_verified: verification_complete,
        verification_complete,
        recoverability: "transparent-lossless-decompression-by-filesystem",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(path: &str, allocated_bytes: u64) -> TransparentCompressionCandidate {
        TransparentCompressionCandidate {
            path: path.into(),
            logical_bytes: allocated_bytes,
            allocated_bytes,
            modified_ms: 1,
            device: 1,
            inode: allocated_bytes,
        }
    }

    #[test]
    fn partial_worker_failure_preserves_successful_mutation_receipt() {
        let candidates = vec![candidate("/tmp/one.jsonl", 100), candidate("/tmp/two.jsonl", 200)];
        let progress = summarize_compression_results(
            &candidates,
            vec![Ok((40, true)), Err("compression-output-sync-failed".into())],
        );
        assert_eq!(progress.compressed_count, 1);
        assert_eq!(progress.not_compressible_count, 0);
        assert_eq!(progress.failed_count, 1);
        assert_eq!(progress.allocated_bytes_after_upper_bound, 240);
        assert_eq!(progress.failures, vec!["compression-output-sync-failed"]);
    }

    #[test]
    fn activity_probe_fails_closed_on_lsof_errors() {
        assert!(activity_probe_proves_inactive(Some(1), false, false, false));
        assert!(!activity_probe_proves_inactive(Some(0), false, false, false));
        assert!(!activity_probe_proves_inactive(Some(2), false, false, false));
        assert!(!activity_probe_proves_inactive(None, false, false, false));
        assert!(!activity_probe_proves_inactive(Some(1), true, false, false));
        assert!(!activity_probe_proves_inactive(Some(1), false, true, false));
        assert!(!activity_probe_proves_inactive(Some(1), false, false, true));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn empty_plan_withholds_mutation_approval() {
        let temp = tempfile::tempdir().unwrap();
        let planned = plan(temp.path(), 1, 10, crate::cloud::system_now_ms()).unwrap();
        assert_eq!(planned.candidate_count, 0);
        assert!(planned.exact_approval_phrase.is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn plan_is_deterministic_and_only_selects_old_large_jsonl() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("old.jsonl");
        File::create(&path).unwrap().set_len(MIN_BYTES).unwrap();
        let now_ms = crate::cloud::system_now_ms() + 2 * 86_400_000;
        let first = plan(temp.path(), 1, 10, now_ms).unwrap();
        let second = plan(temp.path(), 1, 10, now_ms).unwrap();
        let tighter = plan(temp.path(), 1, 1, now_ms).unwrap();
        assert_eq!(first.candidate_count, 1);
        assert_eq!(first.plan_fingerprint, second.plan_fingerprint);
        assert_ne!(first.plan_fingerprint, tighter.plan_fingerprint);
    }
}
