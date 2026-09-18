//! Log/transcript compression with verify-then-remove (lossless zstd).
//!
//! Compresses regular files under a required root. `--older-than-days N` (N≥1) keeps an
//! optional age filter; `--older-than-days 0` disables the age gate (Codex session history
//! and similar lossless archives — not a deletion, so not subject to the Orca 7d reclaim
//! recent-write gate). `--min-stable-secs` still refuses recently-touched files.
//! Archives keep the original path and name with a `.zst` suffix. The original is removed
//! only after `zstd -t` and a decompressed SHA-256 match succeed. Dry-run is the default;
//! execution journals every file decision.

use crate::cloud_app_managed::app_managed_library_blocker;
use crate::safety::{self, JournalEntry};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

pub const LOG_ARCHIVE_SCHEMA_KIND: &str = "disksage.log-archive/v1";
pub const DEFAULT_MIN_STABLE_SECS: u64 = 3_600;
pub const DEFAULT_EXCLUDE_SUFFIXES: &[&str] = &[
    ".zst",
    ".gz",
    ".xz",
    ".bz2",
    ".zip",
    ".sqlite",
    ".sqlite3",
    ".sqlite-wal",
    ".sqlite-shm",
    ".db",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogArchiveOptions {
    pub root: PathBuf,
    pub older_than_days: u64,
    pub execute: bool,
    pub journal_path: PathBuf,
    pub min_stable_secs: u64,
    pub zstd_bin: PathBuf,
    pub exclude_suffixes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    NotRegularFile,
    Symlink,
    AlreadyArchivedSibling,
    ExcludedSuffix,
    AppManaged(&'static str),
    ProtectedPath,
    TooRecent,
    AgeBelowThreshold,
    MetadataUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveOutcome {
    Planned,
    Archived,
    Skipped(SkipReason),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArchiveFileResult {
    pub path: String,
    pub bytes_original: u64,
    pub bytes_archive: Option<u64>,
    pub bytes_reclaimed: Option<u64>,
    pub age_days: Option<u64>,
    pub outcome: ArchiveOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogArchiveReport {
    pub schema_kind: &'static str,
    pub root: String,
    pub older_than_days: u64,
    pub executed: bool,
    pub zstd_bin: String,
    pub candidates: u64,
    pub archived: u64,
    pub skipped: u64,
    pub failed: u64,
    pub bytes_original_total: u64,
    pub bytes_archive_total: u64,
    pub bytes_reclaimed_total: u64,
    pub results: Vec<ArchiveFileResult>,
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
}

pub fn resolve_zstd_bin() -> Result<PathBuf, String> {
    for candidate in [
        "/opt/homebrew/bin/zstd",
        "/usr/local/bin/zstd",
        "/usr/bin/zstd",
    ] {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Ok(path);
        }
    }
    which_zstd_on_path()
}

fn which_zstd_on_path() -> Result<PathBuf, String> {
    let output = Command::new("sh")
        .args(["-c", "command -v zstd"])
        .output()
        .map_err(|_| "zstd-not-found".to_string())?;
    if !output.status.success() {
        return Err("zstd-not-found".into());
    }
    let path = String::from_utf8_lossy(&output.stdout);
    let path = path.trim();
    if path.is_empty() {
        return Err("zstd-not-found".into());
    }
    let path = PathBuf::from(path);
    if path.is_file() {
        Ok(path)
    } else {
        Err("zstd-not-found".into())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| u64::try_from(value.as_millis()).ok())
        .unwrap_or(0)
}

fn age_days(modified: SystemTime, now: SystemTime) -> Option<u64> {
    now.duration_since(modified)
        .ok()
        .map(|duration| duration.as_secs() / 86_400)
}

fn stable_long_enough(modified: SystemTime, now: SystemTime, min_stable_secs: u64) -> bool {
    now.duration_since(modified)
        .ok()
        .is_some_and(|duration| duration >= Duration::from_secs(min_stable_secs))
}

fn has_excluded_suffix(path: &Path, suffixes: &[String]) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    suffixes.iter().any(|suffix| {
        let needle = suffix.to_ascii_lowercase();
        name.ends_with(&needle)
    })
}

fn archive_path_for(path: &Path) -> PathBuf {
    let mut archive = path.as_os_str().to_owned();
    archive.push(".zst");
    PathBuf::from(archive)
}

fn partial_archive_path_for(path: &Path) -> PathBuf {
    let mut partial = archive_path_for(path).into_os_string();
    partial.push(".partial");
    PathBuf::from(partial)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let file = File::open(path).map_err(|error| format!("sha256-open-failed:{error}"))?;
    sha256_reader(BufReader::new(file))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn sha256_reader(mut reader: impl Read) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 64];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("sha256-stream-failed:{error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

fn run_zstd_test(zstd_bin: &Path, archive: &Path) -> Result<(), String> {
    let status = Command::new(zstd_bin)
        .args(["-t", "--quiet"])
        .arg(archive)
        .status()
        .map_err(|error| format!("zstd-test-spawn-failed:{error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("zstd-test-failed".into())
    }
}

fn decompressed_sha256(zstd_bin: &Path, archive: &Path) -> Result<String, String> {
    let mut child = Command::new(zstd_bin)
        .args(["-dc", "--quiet"])
        .arg(archive)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("zstd-decompress-spawn-failed:{error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "zstd-decompress-stdout-missing".to_string())?;
    let digest = sha256_reader(stdout)?;
    let status = child
        .wait()
        .map_err(|error| format!("zstd-decompress-wait-failed:{error}"))?;
    if !status.success() {
        return Err("zstd-decompress-failed".into());
    }
    Ok(digest)
}

fn compress_to_partial(zstd_bin: &Path, source: &Path, partial: &Path) -> Result<(), String> {
    if partial.exists() {
        fs::remove_file(partial).map_err(|error| format!("partial-cleanup-failed:{error}"))?;
    }
    let status = Command::new(zstd_bin)
        .args(["-q", "-f", "-o"])
        .arg(partial)
        .arg(source)
        .status()
        .map_err(|error| format!("zstd-compress-spawn-failed:{error}"))?;
    if status.success() {
        Ok(())
    } else {
        let _ = fs::remove_file(partial);
        Err("zstd-compress-failed".into())
    }
}

fn journal(
    journal_path: &Path,
    op: &str,
    path: &Path,
    bytes: u64,
    outcome: &str,
    ts_ms: u64,
) -> Result<(), String> {
    safety::journal_append(
        journal_path,
        &JournalEntry {
            ts_ms,
            op: op.to_string(),
            path: path.to_string_lossy().into_owned(),
            bytes,
            outcome: outcome.to_string(),
        },
    )
    .map_err(|error| error.to_string())
}

/// Returns `None` when the file is eligible; `Some(skip)` when it should be skipped.
fn skip_reason_for(
    path: &Path,
    options: &LogArchiveOptions,
    now: SystemTime,
) -> Result<Option<(SkipReason, u64, Option<u64>)>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(Some((SkipReason::MetadataUnavailable, 0, None))),
    };
    if metadata.file_type().is_symlink() {
        return Ok(Some((SkipReason::Symlink, 0, None)));
    }
    if !metadata.is_file() {
        return Ok(Some((SkipReason::NotRegularFile, 0, None)));
    }
    let bytes = metadata.len();
    if has_excluded_suffix(path, &options.exclude_suffixes) {
        return Ok(Some((SkipReason::ExcludedSuffix, bytes, None)));
    }
    if archive_path_for(path).exists() {
        return Ok(Some((SkipReason::AlreadyArchivedSibling, bytes, None)));
    }
    if let Some(reason) = app_managed_library_blocker(path) {
        return Ok(Some((SkipReason::AppManaged(reason), bytes, None)));
    }
    if safety::is_protected(path) {
        return Ok(Some((SkipReason::ProtectedPath, bytes, None)));
    }
    let modified = metadata
        .modified()
        .map_err(|_| "mtime-unavailable".to_string())?;
    let age = age_days(modified, now);
    if !stable_long_enough(modified, now, options.min_stable_secs) {
        return Ok(Some((SkipReason::TooRecent, bytes, age)));
    }
    let Some(age_days) = age else {
        return Ok(Some((SkipReason::MetadataUnavailable, bytes, None)));
    };
    // older_than_days == 0: no age gate (lossless compression of all min-stable files).
    if options.older_than_days > 0 && age_days < options.older_than_days {
        return Ok(Some((SkipReason::AgeBelowThreshold, bytes, Some(age_days))));
    }
    Ok(None)
}

fn fingerprint(path: &Path) -> Result<(u64, SystemTime), String> {
    let metadata = fs::metadata(path).map_err(|error| format!("fingerprint-failed:{error}"))?;
    let modified = metadata
        .modified()
        .map_err(|error| format!("fingerprint-mtime-failed:{error}"))?;
    Ok((metadata.len(), modified))
}

fn archive_one(
    path: &Path,
    options: &LogArchiveOptions,
    age_days: u64,
    bytes_original: u64,
    now_ms_value: u64,
) -> ArchiveFileResult {
    let path_string = path.to_string_lossy().into_owned();
    let archive = archive_path_for(path);
    let partial = partial_archive_path_for(path);

    if let Err(error) = journal(
        &options.journal_path,
        "log_archive_compress",
        path,
        bytes_original,
        "pending",
        now_ms_value,
    ) {
        return ArchiveFileResult {
            path: path_string,
            bytes_original,
            bytes_archive: None,
            bytes_reclaimed: None,
            age_days: Some(age_days),
            outcome: ArchiveOutcome::Failed(error),
        };
    }

    let original_fp = match fingerprint(path) {
        Ok(value) => value,
        Err(error) => {
            let _ = journal(
                &options.journal_path,
                "log_archive_compress",
                path,
                bytes_original,
                &format!("failed:{error}"),
                now_ms(),
            );
            return ArchiveFileResult {
                path: path_string,
                bytes_original,
                bytes_archive: None,
                bytes_reclaimed: None,
                age_days: Some(age_days),
                outcome: ArchiveOutcome::Failed(error),
            };
        }
    };

    let original_digest = match sha256_file(path) {
        Ok(value) => value,
        Err(error) => {
            let _ = journal(
                &options.journal_path,
                "log_archive_compress",
                path,
                bytes_original,
                &format!("failed:{error}"),
                now_ms(),
            );
            return ArchiveFileResult {
                path: path_string,
                bytes_original,
                bytes_archive: None,
                bytes_reclaimed: None,
                age_days: Some(age_days),
                outcome: ArchiveOutcome::Failed(error),
            };
        }
    };

    if let Err(error) = compress_to_partial(&options.zstd_bin, path, &partial) {
        let _ = journal(
            &options.journal_path,
            "log_archive_compress",
            path,
            bytes_original,
            &format!("failed:{error}"),
            now_ms(),
        );
        return ArchiveFileResult {
            path: path_string,
            bytes_original,
            bytes_archive: None,
            bytes_reclaimed: None,
            age_days: Some(age_days),
            outcome: ArchiveOutcome::Failed(error),
        };
    }

    if let Err(error) = fs::rename(&partial, &archive) {
        let _ = fs::remove_file(&partial);
        let message = format!("archive-rename-failed:{error}");
        let _ = journal(
            &options.journal_path,
            "log_archive_compress",
            path,
            bytes_original,
            &format!("failed:{message}"),
            now_ms(),
        );
        return ArchiveFileResult {
            path: path_string,
            bytes_original,
            bytes_archive: None,
            bytes_reclaimed: None,
            age_days: Some(age_days),
            outcome: ArchiveOutcome::Failed(message),
        };
    }

    if let Err(error) = run_zstd_test(&options.zstd_bin, &archive) {
        let _ = fs::remove_file(&archive);
        let _ = journal(
            &options.journal_path,
            "log_archive_compress",
            path,
            bytes_original,
            &format!("failed:{error}"),
            now_ms(),
        );
        return ArchiveFileResult {
            path: path_string,
            bytes_original,
            bytes_archive: None,
            bytes_reclaimed: None,
            age_days: Some(age_days),
            outcome: ArchiveOutcome::Failed(error),
        };
    }

    let decompressed = match decompressed_sha256(&options.zstd_bin, &archive) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(&archive);
            let _ = journal(
                &options.journal_path,
                "log_archive_compress",
                path,
                bytes_original,
                &format!("failed:{error}"),
                now_ms(),
            );
            return ArchiveFileResult {
                path: path_string,
                bytes_original,
                bytes_archive: None,
                bytes_reclaimed: None,
                age_days: Some(age_days),
                outcome: ArchiveOutcome::Failed(error),
            };
        }
    };

    if decompressed != original_digest {
        let _ = fs::remove_file(&archive);
        let message = "sha256-mismatch-after-decompress".to_string();
        let _ = journal(
            &options.journal_path,
            "log_archive_compress",
            path,
            bytes_original,
            &format!("failed:{message}"),
            now_ms(),
        );
        return ArchiveFileResult {
            path: path_string,
            bytes_original,
            bytes_archive: None,
            bytes_reclaimed: None,
            age_days: Some(age_days),
            outcome: ArchiveOutcome::Failed(message),
        };
    }

    let current_fp = match fingerprint(path) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(&archive);
            let _ = journal(
                &options.journal_path,
                "log_archive_compress",
                path,
                bytes_original,
                &format!("failed:{error}"),
                now_ms(),
            );
            return ArchiveFileResult {
                path: path_string,
                bytes_original,
                bytes_archive: None,
                bytes_reclaimed: None,
                age_days: Some(age_days),
                outcome: ArchiveOutcome::Failed(error),
            };
        }
    };
    if current_fp != original_fp {
        let _ = fs::remove_file(&archive);
        let message = "live-write-detected-before-remove".to_string();
        let _ = journal(
            &options.journal_path,
            "log_archive_compress",
            path,
            bytes_original,
            &format!("failed:{message}"),
            now_ms(),
        );
        return ArchiveFileResult {
            path: path_string,
            bytes_original,
            bytes_archive: None,
            bytes_reclaimed: None,
            age_days: Some(age_days),
            outcome: ArchiveOutcome::Failed(message),
        };
    }

    let archive_bytes = fs::metadata(&archive).map(|metadata| metadata.len()).unwrap_or(0);
    if let Err(error) = fs::remove_file(path) {
        let message = format!("original-remove-failed:{error}");
        let _ = journal(
            &options.journal_path,
            "log_archive_compress",
            path,
            bytes_original,
            &format!("failed:{message}"),
            now_ms(),
        );
        return ArchiveFileResult {
            path: path_string,
            bytes_original,
            bytes_archive: Some(archive_bytes),
            bytes_reclaimed: None,
            age_days: Some(age_days),
            outcome: ArchiveOutcome::Failed(message),
        };
    }

    let reclaimed = bytes_original.saturating_sub(archive_bytes);
    let _ = journal(
        &options.journal_path,
        "log_archive_compress",
        path,
        reclaimed,
        "ok",
        now_ms(),
    );

    ArchiveFileResult {
        path: path_string,
        bytes_original,
        bytes_archive: Some(archive_bytes),
        bytes_reclaimed: Some(reclaimed),
        age_days: Some(age_days),
        outcome: ArchiveOutcome::Archived,
    }
}

pub fn validate_options(options: &LogArchiveOptions) -> Result<(), String> {
    if !absolute_without_parent(&options.root) {
        return Err("--root must be an absolute path without parent segments".into());
    }
    if !options.root.is_dir() {
        return Err("--root must be an existing directory".into());
    }
    if options.older_than_days > 3_650 {
        return Err("--older-than-days must be between 0 and 3650 (0 disables age gate)".into());
    }
    if !absolute_without_parent(&options.journal_path) {
        return Err("--journal-path must be an absolute path without parent segments".into());
    }
    if options.min_stable_secs == 0 {
        return Err("--min-stable-secs must be >= 1".into());
    }
    if !options.zstd_bin.is_file() {
        return Err("zstd-bin-missing".into());
    }
    Ok(())
}

pub fn archive_logs(options: &LogArchiveOptions) -> Result<LogArchiveReport, String> {
    validate_options(options)?;
    if options.execute {
        if let Some(parent) = options.journal_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }

    let now = SystemTime::now();
    let mut results = Vec::new();
    let mut candidates = 0_u64;
    let mut archived = 0_u64;
    let mut skipped = 0_u64;
    let mut failed = 0_u64;
    let mut bytes_original_total = 0_u64;
    let mut bytes_archive_total = 0_u64;
    let mut bytes_reclaimed_total = 0_u64;

    for entry in WalkDir::new(&options.root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path == options.root || entry.file_type().is_dir() {
            continue;
        }
        match skip_reason_for(path, options, now)? {
            Some((reason, bytes, age)) => {
                skipped += 1;
                results.push(ArchiveFileResult {
                    path: path.to_string_lossy().into_owned(),
                    bytes_original: bytes,
                    bytes_archive: None,
                    bytes_reclaimed: None,
                    age_days: age,
                    outcome: ArchiveOutcome::Skipped(reason),
                });
            }
            None => {
                let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
                let bytes = metadata.len();
                let modified = metadata.modified().map_err(|error| error.to_string())?;
                let age = age_days(modified, now).unwrap_or(0);
                candidates += 1;
                bytes_original_total += bytes;
                if options.execute {
                    let result = archive_one(path, options, age, bytes, now_ms());
                    match &result.outcome {
                        ArchiveOutcome::Archived => {
                            archived += 1;
                            if let Some(archive_bytes) = result.bytes_archive {
                                bytes_archive_total += archive_bytes;
                            }
                            if let Some(reclaimed) = result.bytes_reclaimed {
                                bytes_reclaimed_total += reclaimed;
                            }
                        }
                        ArchiveOutcome::Failed(_) => failed += 1,
                        ArchiveOutcome::Skipped(_) | ArchiveOutcome::Planned => {}
                    }
                    results.push(result);
                } else {
                    results.push(ArchiveFileResult {
                        path: path.to_string_lossy().into_owned(),
                        bytes_original: bytes,
                        bytes_archive: None,
                        bytes_reclaimed: None,
                        age_days: Some(age),
                        outcome: ArchiveOutcome::Planned,
                    });
                }
            }
        }
    }

    Ok(LogArchiveReport {
        schema_kind: LOG_ARCHIVE_SCHEMA_KIND,
        root: options.root.to_string_lossy().into_owned(),
        older_than_days: options.older_than_days,
        executed: options.execute,
        zstd_bin: options.zstd_bin.to_string_lossy().into_owned(),
        candidates,
        archived,
        skipped,
        failed,
        bytes_original_total,
        bytes_archive_total,
        bytes_reclaimed_total,
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn touch_aged(path: &Path, age_days: u64) {
        let modified = SystemTime::now()
            .checked_sub(Duration::from_secs(age_days.saturating_mul(86_400)))
            .unwrap_or(UNIX_EPOCH);
        let file = File::options().write(true).open(path).unwrap();
        file.set_modified(modified).unwrap();
    }

    fn options_for(root: &Path, journal: &Path, execute: bool) -> LogArchiveOptions {
        LogArchiveOptions {
            root: root.to_path_buf(),
            older_than_days: 30,
            execute,
            journal_path: journal.to_path_buf(),
            min_stable_secs: 1,
            zstd_bin: resolve_zstd_bin().expect("zstd required for tests"),
            exclude_suffixes: DEFAULT_EXCLUDE_SUFFIXES
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        }
    }

    #[test]
    fn dry_run_plans_old_text_and_skips_sqlite_and_recent() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("logs");
        fs::create_dir_all(&root).unwrap();
        let old = root.join("old.jsonl");
        let recent = root.join("recent.jsonl");
        let sqlite = root.join("state.sqlite");
        fs::write(&old, b"{\"session\":1}\n".repeat(200)).unwrap();
        fs::write(&recent, b"recent\n").unwrap();
        fs::write(&sqlite, b"sqlite").unwrap();
        touch_aged(&old, 40);
        touch_aged(&recent, 2);
        touch_aged(&sqlite, 40);

        let journal = tmp.path().join("journal.jsonl");
        let report = archive_logs(&options_for(&root, &journal, false)).unwrap();
        assert!(!report.executed);
        assert_eq!(report.candidates, 1);
        assert!(report.results.iter().any(|result| {
            result.path.ends_with("old.jsonl")
                && matches!(result.outcome, ArchiveOutcome::Planned)
        }));
        assert!(report.results.iter().any(|result| {
            result.path.ends_with("state.sqlite")
                && matches!(
                    result.outcome,
                    ArchiveOutcome::Skipped(SkipReason::ExcludedSuffix)
                )
        }));
        assert!(report.results.iter().any(|result| {
            result.path.ends_with("recent.jsonl")
                && matches!(
                    result.outcome,
                    ArchiveOutcome::Skipped(SkipReason::AgeBelowThreshold)
                )
        }));
        assert!(!archive_path_for(&old).exists());
        assert!(old.exists());
    }

    #[test]
    fn execute_verifies_then_removes_original() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("logs");
        fs::create_dir_all(&root).unwrap();
        let old = root.join("transcript.jsonl");
        let mut file = fs::File::create(&old).unwrap();
        for index in 0..2_000 {
            writeln!(file, r#"{{"i":{index},"msg":"hello world compress me"}}"#).unwrap();
        }
        drop(file);
        touch_aged(&old, 45);

        let journal = tmp.path().join("journal.jsonl");
        let report = archive_logs(&options_for(&root, &journal, true)).unwrap();
        assert_eq!(report.archived, 1);
        assert!(!old.exists());
        let archive = PathBuf::from(format!("{}.zst", old.display()));
        assert!(archive.is_file());
        let journal_text = fs::read_to_string(&journal).unwrap();
        assert!(journal_text.contains("log_archive_compress"));
        assert!(journal_text.contains("\"outcome\":\"ok\""));
        assert!(report.bytes_reclaimed_total > 0);
    }

    #[test]
    fn refuses_app_managed_parallels_paths() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("Parallels").join("Linux.macvm");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("old.log");
        fs::write(&file, b"vm-log").unwrap();
        touch_aged(&file, 90);
        let journal = tmp.path().join("journal.jsonl");
        let report = archive_logs(&options_for(&root, &journal, false)).unwrap();
        assert_eq!(report.candidates, 0);
        assert!(report.results.iter().any(|result| {
            matches!(
                result.outcome,
                ArchiveOutcome::Skipped(SkipReason::AppManaged(_))
            )
        }));
    }
}
