//! Log/transcript compression with verify-then-trash (lossless zstd).
//!
//! Compresses regular files under a required root. `--older-than-days N` (N≥1) keeps an
//! optional age filter; `--older-than-days 0` disables the age gate (Codex session history
//! and similar lossless archives). This path still retires sources through DiskSage's
//! identity-bound OS Trash contract — it is not a permanent-delete exception and is not a
//! substitute for measured physical reclaim. `--min-stable-secs` still refuses
//! recently-touched files. Archives keep the original path and name with a `.zst` suffix.
//! The reviewed source is trashed only after `zstd -t`, a decompressed SHA-256 match, and a
//! filesystem-object identity revalidation succeed. Dry-run is the default; execution
//! journals each compression operation, while dry-run and skipped decisions remain in the
//! returned report only.

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
    let path_var = std::env::var_os("PATH").ok_or_else(|| "zstd-not-found".to_string())?;
    for dir in std::env::split_paths(&path_var) {
        for name in ["zstd", "zstd.exe"] {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err("zstd-not-found".into())
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
    partial.push(format!(
        ".partial.{}.{}",
        std::process::id(),
        now_ms()
    ));
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
    // Atomically reserve the partial (create-new). Existence pre-check + `zstd -o` is TOCTOU;
    // own the path first, then direct zstd stdout into the reserved file.
    let reserved = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(partial)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "partial-already-exists".into()
            } else {
                format!("partial-create-failed:{error}")
            }
        })?;
    let status = Command::new(zstd_bin)
        .args(["-q", "-c"])
        .arg(source)
        .stdout(Stdio::from(reserved))
        .stderr(Stdio::null())
        .status()
        .map_err(|error| {
            let _ = fs::remove_file(partial);
            format!("zstd-compress-spawn-failed:{error}")
        })?;
    if status.success() {
        Ok(())
    } else {
        let _ = fs::remove_file(partial);
        Err("zstd-compress-failed".into())
    }
}

/// Publish `.zst` without clobbering a concurrent/existing archive (`rename` would replace).
fn publish_archive_no_clobber(partial: &Path, archive: &Path) -> Result<(), String> {
    match fs::hard_link(partial, archive) {
        Ok(()) => {
            if let Err(error) = fs::remove_file(partial) {
                let _ = fs::remove_file(archive);
                return Err(format!("partial-unlink-failed:{error}"));
            }
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(partial);
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                Err("archive-already-exists".into())
            } else {
                Err(format!("archive-publish-failed:{error}"))
            }
        }
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

/// Returns true only when the newest log-archive ledger record for this path proves that DiskSage
/// itself had already verified an archive and reached the reversible source-retirement boundary.
fn retirement_retry_authorized(journal_path: &Path, path: &Path) -> bool {
    let path_string = path.to_string_lossy();
    safety::journal_recent(journal_path, usize::MAX)
        .into_iter()
        .find(|entry| entry.op == "log_archive_compress" && entry.path == path_string)
        .is_some_and(|entry| {
            entry.outcome == "archive_verified_retirement_pending"
                || entry.outcome.starts_with("failed:original-trash-failed:")
        })
}

/// Returns `None` when the file is eligible; `Some(skip)` when it should be skipped.
fn skip_reason_for(
    path: &Path,
    options: &LogArchiveOptions,
    now: SystemTime,
) -> Option<(SkipReason, u64, Option<u64>)> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return Some((SkipReason::MetadataUnavailable, 0, None)),
    };
    if metadata.file_type().is_symlink() {
        return Some((SkipReason::Symlink, 0, None));
    }
    if !metadata.is_file() {
        return Some((SkipReason::NotRegularFile, 0, None));
    }
    let bytes = metadata.len();
    if has_excluded_suffix(path, &options.exclude_suffixes) {
        return Some((SkipReason::ExcludedSuffix, bytes, None));
    }
    if archive_path_for(path).exists()
        && (!options.execute || !retirement_retry_authorized(&options.journal_path, path))
    {
        return Some((SkipReason::AlreadyArchivedSibling, bytes, None));
    }
    if let Some(reason) = app_managed_library_blocker(path) {
        return Some((SkipReason::AppManaged(reason), bytes, None));
    }
    if safety::is_protected(path) {
        return Some((SkipReason::ProtectedPath, bytes, None));
    }
    let modified = match metadata.modified() {
        Ok(modified) => modified,
        Err(_) => return Some((SkipReason::MetadataUnavailable, bytes, None)),
    };
    let age = age_days(modified, now);
    if !stable_long_enough(modified, now, options.min_stable_secs) {
        return Some((SkipReason::TooRecent, bytes, age));
    }
    let Some(age_days) = age else {
        return Some((SkipReason::MetadataUnavailable, bytes, None));
    };
    // older_than_days == 0: no age gate (lossless compression of all min-stable files).
    if options.older_than_days > 0 && age_days < options.older_than_days {
        return Some((SkipReason::AgeBelowThreshold, bytes, Some(age_days)));
    }
    None
}

fn capture_object_id(path: &Path) -> Result<String, String> {
    safety::filesystem_object_id(path)
        .map_err(|error| format!("object-identity-unavailable:{error}"))
}

fn mutation_blocked(path: &Path) -> Option<String> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Some("parent-dir-segment".into());
    }
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            return Some("symlink-at-mutation".into());
        }
    }
    if let Some(reason) = app_managed_library_blocker(path) {
        return Some(format!("app-managed:{reason}"));
    }
    // Fail closed: lexical fallback would bypass protected-root authority on canonicalize errors.
    let canonical = match fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(_) => return Some("canonicalize-failed".into()),
    };
    if safety::is_protected(&canonical) || safety::is_protected(path) {
        return Some("protected-path".into());
    }
    None
}

/// Resume only a DiskSage-ledger-correlated retirement after re-verifying both the durable archive
/// and the unchanged source. Pre-existing archives are never overwritten or removed on this path.
fn resume_verified_archive_retirement(
    path: &Path,
    options: &LogArchiveOptions,
    age_days: u64,
    bytes_original: u64,
) -> ArchiveFileResult {
    let path_string = path.to_string_lossy().into_owned();
    let archive = archive_path_for(path);
    let fail = |message: String, bytes_archive: Option<u64>| ArchiveFileResult {
        path: path_string.clone(),
        bytes_original,
        bytes_archive,
        bytes_reclaimed: None,
        age_days: Some(age_days),
        outcome: ArchiveOutcome::Failed(message),
    };

    if let Some(reason) = mutation_blocked(path) {
        return fail(format!("mutation-authority-rejected:{reason}"), None);
    }

    let archive_metadata = match fs::symlink_metadata(&archive) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => metadata,
        Ok(_) => return fail("existing-archive-not-regular-file".into(), None),
        Err(error) => return fail(format!("existing-archive-metadata-failed:{error}"), None),
    };
    let archive_bytes = archive_metadata.len();
    let archive_object_id = match capture_object_id(&archive) {
        Ok(value) => value,
        Err(error) => return fail(format!("existing-archive-{error}"), Some(archive_bytes)),
    };
    let original_object_id = match capture_object_id(path) {
        Ok(value) => value,
        Err(error) => return fail(error, Some(archive_bytes)),
    };
    let original_digest = match sha256_file(path) {
        Ok(value) => value,
        Err(error) => return fail(error, Some(archive_bytes)),
    };

    if let Err(error) = run_zstd_test(&options.zstd_bin, &archive) {
        return fail(format!("existing-archive-unverified:{error}"), Some(archive_bytes));
    }
    let decompressed = match decompressed_sha256(&options.zstd_bin, &archive) {
        Ok(value) => value,
        Err(error) => {
            return fail(
                format!("existing-archive-unverified:{error}"),
                Some(archive_bytes),
            )
        }
    };
    if decompressed != original_digest {
        return fail("existing-archive-content-mismatch".into(), Some(archive_bytes));
    }

    if let Some(reason) = mutation_blocked(path) {
        return fail(
            format!("mutation-authority-rejected:{reason}"),
            Some(archive_bytes),
        );
    }
    match capture_object_id(path) {
        Ok(current) if current == original_object_id => {}
        Ok(_) => return fail("object-identity-changed-before-retire".into(), Some(archive_bytes)),
        Err(error) => return fail(error, Some(archive_bytes)),
    }
    match sha256_file(path) {
        Ok(live_digest) if live_digest == original_digest => {}
        Ok(_) => return fail("live-content-changed-before-retire".into(), Some(archive_bytes)),
        Err(error) => return fail(error, Some(archive_bytes)),
    }
    match capture_object_id(&archive) {
        Ok(current) if current == archive_object_id => {}
        Ok(_) => return fail("existing-archive-object-changed".into(), Some(archive_bytes)),
        Err(error) => {
            return fail(
                format!("existing-archive-{error}"),
                Some(archive_bytes),
            )
        }
    }
    match decompressed_sha256(&options.zstd_bin, &archive) {
        Ok(live_digest) if live_digest == original_digest => {}
        Ok(_) => return fail("existing-archive-content-changed".into(), Some(archive_bytes)),
        Err(error) => {
            return fail(
                format!("existing-archive-unverified:{error}"),
                Some(archive_bytes),
            )
        }
    }

    if let Err(error) = journal(
        &options.journal_path,
        "log_archive_retirement_retry",
        path,
        bytes_original,
        "archive_verified_retirement_pending",
        now_ms(),
    ) {
        return fail(error, Some(archive_bytes));
    }
    if let Err(error) = safety::trash_delete_if_identity(
        path,
        &original_object_id,
        bytes_original,
        &options.journal_path,
        now_ms(),
    ) {
        let message = format!("original-trash-failed:{error}");
        let _ = journal(
            &options.journal_path,
            "log_archive_retirement_retry",
            path,
            bytes_original,
            &format!("failed:{message}"),
            now_ms(),
        );
        return fail(message, Some(archive_bytes));
    }
    let _ = journal(
        &options.journal_path,
        "log_archive_retirement_retry",
        path,
        bytes_original,
        "ok",
        now_ms(),
    );

    ArchiveFileResult {
        path: path_string,
        bytes_original,
        bytes_archive: Some(archive_bytes),
        bytes_reclaimed: None,
        age_days: Some(age_days),
        outcome: ArchiveOutcome::Archived,
    }
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

    let fail = |message: String, bytes_archive: Option<u64>| ArchiveFileResult {
        path: path_string.clone(),
        bytes_original,
        bytes_archive,
        bytes_reclaimed: None,
        age_days: Some(age_days),
        outcome: ArchiveOutcome::Failed(message),
    };

    if let Err(error) = journal(
        &options.journal_path,
        "log_archive_compress",
        path,
        bytes_original,
        "pending",
        now_ms_value,
    ) {
        return fail(error, None);
    }

    if let Some(reason) = mutation_blocked(path) {
        let message = format!("mutation-authority-rejected:{reason}");
        let _ = journal(
            &options.journal_path,
            "log_archive_compress",
            path,
            bytes_original,
            &format!("failed:{message}"),
            now_ms(),
        );
        return fail(message, None);
    }

    if archive.exists() {
        let message = "archive-already-exists".to_string();
        let _ = journal(
            &options.journal_path,
            "log_archive_compress",
            path,
            bytes_original,
            &format!("failed:{message}"),
            now_ms(),
        );
        return fail(message, None);
    }

    let original_object_id = match capture_object_id(path) {
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
            return fail(error, None);
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
            return fail(error, None);
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
        return fail(error, None);
    }

    if let Err(error) = publish_archive_no_clobber(&partial, &archive) {
        let _ = journal(
            &options.journal_path,
            "log_archive_compress",
            path,
            bytes_original,
            &format!("failed:{error}"),
            now_ms(),
        );
        return fail(error, None);
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
        return fail(error, None);
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
            return fail(error, None);
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
        return fail(message, None);
    }

    // Fail closed if the reviewed object was replaced, mutated in place, or lost authority.
    if let Some(reason) = mutation_blocked(path) {
        let _ = fs::remove_file(&archive);
        let message = format!("mutation-authority-rejected:{reason}");
        let _ = journal(
            &options.journal_path,
            "log_archive_compress",
            path,
            bytes_original,
            &format!("failed:{message}"),
            now_ms(),
        );
        return fail(message, None);
    }

    let current_object_id = match capture_object_id(path) {
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
            return fail(error, None);
        }
    };
    if current_object_id != original_object_id {
        let _ = fs::remove_file(&archive);
        let message = "object-identity-changed-before-retire".to_string();
        let _ = journal(
            &options.journal_path,
            "log_archive_compress",
            path,
            bytes_original,
            &format!("failed:{message}"),
            now_ms(),
        );
        return fail(message, None);
    }

    // Same inode can still be rewritten in place; refuse to trash drifted content.
    match sha256_file(path) {
        Ok(live_digest) if live_digest == original_digest => {}
        Ok(_) => {
            let _ = fs::remove_file(&archive);
            let message = "live-content-changed-before-retire".to_string();
            let _ = journal(
                &options.journal_path,
                "log_archive_compress",
                path,
                bytes_original,
                &format!("failed:{message}"),
                now_ms(),
            );
            return fail(message, None);
        }
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
            return fail(error, None);
        }
    }

    let archive_bytes = fs::metadata(&archive).map(|metadata| metadata.len()).unwrap_or(0);
    if let Err(error) = journal(
        &options.journal_path,
        "log_archive_compress",
        path,
        bytes_original,
        "archive_verified_retirement_pending",
        now_ms(),
    ) {
        let _ = fs::remove_file(&archive);
        return fail(error, Some(archive_bytes));
    }
    if let Err(error) = safety::trash_delete_if_identity(
        path,
        &original_object_id,
        bytes_original,
        &options.journal_path,
        now_ms(),
    ) {
        let message = format!("original-trash-failed:{error}");
        let _ = journal(
            &options.journal_path,
            "log_archive_compress",
            path,
            bytes_original,
            &format!("failed:{message}"),
            now_ms(),
        );
        return fail(message, Some(archive_bytes));
    }

    // Physical free bytes are unknown while Trash retains the object; do not credit
    // logical source−archive as reclaimed space.
    let _ = journal(
        &options.journal_path,
        "log_archive_compress",
        path,
        bytes_original,
        "ok",
        now_ms(),
    );

    ArchiveFileResult {
        path: path_string,
        bytes_original,
        bytes_archive: Some(archive_bytes),
        bytes_reclaimed: None,
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

fn metadata_unavailable_result(path: &Path, bytes_original: u64) -> ArchiveFileResult {
    ArchiveFileResult {
        path: path.to_string_lossy().into_owned(),
        bytes_original,
        bytes_archive: None,
        bytes_reclaimed: None,
        age_days: None,
        outcome: ArchiveOutcome::Skipped(SkipReason::MetadataUnavailable),
    }
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
        .filter_entry(crate::scanner::keep_entry)
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path == options.root || path == options.journal_path || entry.file_type().is_dir() {
            continue;
        }
        match skip_reason_for(path, options, now) {
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
                let metadata = match fs::metadata(path) {
                    Ok(metadata) => metadata,
                    Err(_) => {
                        skipped += 1;
                        results.push(metadata_unavailable_result(path, 0));
                        continue;
                    }
                };
                let bytes = metadata.len();
                let modified = match metadata.modified() {
                    Ok(modified) => modified,
                    Err(_) => {
                        skipped += 1;
                        results.push(metadata_unavailable_result(path, bytes));
                        continue;
                    }
                };
                let age = age_days(modified, now).unwrap_or(0);
                candidates += 1;
                bytes_original_total += bytes;
                if options.execute {
                    let result = if archive_path_for(path).exists() {
                        resume_verified_archive_retirement(path, options, age, bytes)
                    } else {
                        archive_one(path, options, age, bytes, now_ms())
                    };
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
                        ArchiveOutcome::Skipped(_) => skipped += 1,
                        ArchiveOutcome::Planned => {}
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
        assert!(!journal.exists());
    }

    #[test]
    fn journal_inside_root_is_excluded_from_the_report() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("logs");
        fs::create_dir_all(&root).unwrap();
        let journal = root.join("journal.jsonl");
        fs::write(&journal, b"existing journal\n").unwrap();
        touch_aged(&journal, 40);

        let report = archive_logs(&options_for(&root, &journal, true)).unwrap();

        assert_eq!(report.candidates, 0);
        assert_eq!(report.skipped, 0);
        assert!(report.results.is_empty());
        assert_eq!(fs::read(&journal).unwrap(), b"existing journal\n");
    }

    #[test]
    fn execute_verifies_then_trashes_original_without_crediting_reclaim() {
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
        assert!(journal_text.contains("trash_delete"));
        assert!(journal_text.contains("\"outcome\":\"ok\""));
        assert_eq!(
            report.bytes_reclaimed_total, 0,
            "Trash retention means physical reclaim is not credited from logical sizes"
        );
        assert!(report.results.iter().all(|result| result.bytes_reclaimed.is_none()));
    }

    #[test]
    fn existing_archive_is_never_clobbered() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("logs");
        fs::create_dir_all(&root).unwrap();
        let old = root.join("session.jsonl");
        fs::write(&old, b"{\"session\":1}\n".repeat(200)).unwrap();
        touch_aged(&old, 40);
        let archive = archive_path_for(&old);
        fs::write(&archive, b"preexisting-archive").unwrap();

        let journal = tmp.path().join("journal.jsonl");
        let report = archive_logs(&options_for(&root, &journal, true)).unwrap();
        assert_eq!(report.archived, 0);
        assert!(old.exists());
        assert_eq!(fs::read(&archive).unwrap(), b"preexisting-archive");
        assert!(report.results.iter().any(|result| {
            matches!(
                result.outcome,
                ArchiveOutcome::Skipped(SkipReason::AlreadyArchivedSibling)
            )
        }));
    }

    #[cfg(unix)]
    #[test]
    fn hardlink_sibling_outside_root_survives_source_retirement() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("logs");
        fs::create_dir_all(&root).unwrap();
        let old = root.join("shared.jsonl");
        let sibling = tmp.path().join("shared-link.jsonl");
        fs::write(&old, b"{\"session\":1}\n".repeat(200)).unwrap();
        fs::hard_link(&old, &sibling).unwrap();
        touch_aged(&old, 40);

        let journal = tmp.path().join("journal.jsonl");
        let report = archive_logs(&options_for(&root, &journal, true)).unwrap();
        assert_eq!(report.archived, 1);
        assert!(!old.exists());
        assert!(sibling.exists(), "hardlink sibling must retain shared inode content");
        assert_eq!(
            fs::read(&sibling).unwrap(),
            b"{\"session\":1}\n".repeat(200)
        );
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

    #[test]
    fn compress_to_partial_uses_create_new_and_preserves_preexisting() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("session.jsonl");
        fs::write(&source, b"{\"session\":1}\n".repeat(200)).unwrap();
        let partial = tmp.path().join("session.jsonl.zst.partial.reserved");
        fs::write(&partial, b"foreign-partial").unwrap();
        let zstd = resolve_zstd_bin().expect("zstd required for tests");
        let error = compress_to_partial(&zstd, &source, &partial).unwrap_err();
        assert_eq!(error, "partial-already-exists");
        assert_eq!(fs::read(&partial).unwrap(), b"foreign-partial");
    }

    #[test]
    fn mutation_blocked_fail_closed_when_canonicalize_fails() {
        let missing = tempdir()
            .unwrap()
            .path()
            .join("never-created")
            .join("session.jsonl");
        assert_eq!(
            mutation_blocked(&missing).as_deref(),
            Some("canonicalize-failed")
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_is_skipped_and_not_followed_for_mutation() {
        use std::os::unix::fs::symlink;

        let tmp = tempdir().unwrap();
        let root = tmp.path().join("logs");
        fs::create_dir_all(&root).unwrap();
        let target = tmp.path().join("outside.jsonl");
        fs::write(&target, b"{\"session\":1}\n".repeat(200)).unwrap();
        touch_aged(&target, 40);
        let link = root.join("session.jsonl");
        symlink(&target, &link).unwrap();

        let journal = tmp.path().join("journal.jsonl");
        let report = archive_logs(&options_for(&root, &journal, true)).unwrap();
        assert_eq!(report.archived, 0);
        assert_eq!(report.candidates, 0);
        assert!(target.exists(), "symlink target must not be retired");
        assert!(
            link.symlink_metadata().unwrap().file_type().is_symlink(),
            "symlink entry must remain a symlink"
        );
        assert!(report.results.iter().any(|result| {
            matches!(
                result.outcome,
                ArchiveOutcome::Skipped(SkipReason::Symlink)
            )
        }));
        assert_eq!(
            mutation_blocked(&link).as_deref(),
            Some("symlink-at-mutation")
        );
    }

    #[cfg(unix)]
    #[test]
    fn permission_denied_source_fails_closed_without_retirement() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempdir().unwrap();
        let root = tmp.path().join("logs");
        fs::create_dir_all(&root).unwrap();
        let old = root.join("locked.jsonl");
        fs::write(&old, b"{\"session\":1}\n".repeat(200)).unwrap();
        touch_aged(&old, 40);
        let mut permissions = fs::metadata(&old).unwrap().permissions();
        permissions.set_mode(0o000);
        fs::set_permissions(&old, permissions).unwrap();

        let journal = tmp.path().join("journal.jsonl");
        let report = archive_logs(&options_for(&root, &journal, true)).unwrap();

        let mut restore = fs::metadata(&old).unwrap().permissions();
        restore.set_mode(0o600);
        fs::set_permissions(&old, restore).unwrap();

        assert_eq!(report.archived, 0);
        assert!(old.exists(), "unreadable source must not be trashed");
        assert!(!archive_path_for(&old).exists());
        assert!(report.results.iter().any(|result| {
            matches!(
                &result.outcome,
                ArchiveOutcome::Failed(message) if message.contains("sha256-open-failed")
                    || message.contains("mutation-authority-rejected")
                    || message.contains("object-identity-unavailable")
            )
        }));
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_verification_removes_published_archive_and_keeps_source() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempdir().unwrap();
        let root = tmp.path().join("logs");
        fs::create_dir_all(&root).unwrap();
        let old = root.join("session.jsonl");
        let payload = b"{\"session\":1}\n".repeat(200);
        fs::write(&old, &payload).unwrap();
        touch_aged(&old, 40);

        let fake_zstd = tmp.path().join("fake-zstd-verify-fail");
        fs::write(
            &fake_zstd,
            "#!/bin/sh\nset -eu\ncase \"$1\" in\n  -q)\n    cat \"$3\"\n    ;;\n  -t)\n    exit 1\n    ;;\n  -dc)\n    cat \"$3\"\n    ;;\n  *)\n    exit 64\n    ;;\nesac\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&fake_zstd).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_zstd, permissions).unwrap();

        let journal = tmp.path().join("journal.jsonl");
        let options = LogArchiveOptions {
            root: root.clone(),
            older_than_days: 30,
            execute: true,
            journal_path: journal,
            min_stable_secs: 1,
            zstd_bin: fake_zstd,
            exclude_suffixes: DEFAULT_EXCLUDE_SUFFIXES
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        };
        let report = archive_logs(&options).unwrap();
        assert_eq!(report.archived, 0);
        assert_eq!(report.failed, 1);
        assert!(old.exists(), "source must survive interrupted verification");
        assert_eq!(fs::read(&old).unwrap(), payload);
        assert!(
            !archive_path_for(&old).exists(),
            "failed verification must not leave a published archive"
        );
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .all(|entry| entry.file_name() == "session.jsonl"),
            "partial artifacts must be cleaned after interrupted verification"
        );
        assert!(report.results.iter().any(|result| {
            matches!(
                &result.outcome,
                ArchiveOutcome::Failed(message) if message.contains("zstd-test-failed")
            )
        }));
    }

    #[cfg(windows)]
    #[test]
    fn windows_junction_is_not_traversed_for_archive_mutation() {
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("logs");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let outside_file = outside.join("outside.jsonl");
        fs::write(&outside_file, b"{\"outside\":true}\n".repeat(200)).unwrap();
        touch_aged(&outside_file, 40);

        let junction = root.join("outside-junction");
        let output = std::process::Command::new("cmd")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&outside)
            .output()
            .expect("cmd /C mklink /J must be available for the Windows junction fixture");
        assert!(
            output.status.success(),
            "failed to create Windows junction: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let journal = tmp.path().join("journal.jsonl");
        let options = LogArchiveOptions {
            root: root.clone(),
            older_than_days: 30,
            execute: true,
            journal_path: journal,
            min_stable_secs: 1,
            zstd_bin: std::env::current_exe().expect("current test executable is a real file"),
            exclude_suffixes: DEFAULT_EXCLUDE_SUFFIXES
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        };

        let report = archive_logs(&options).unwrap();
        assert_eq!(report.candidates, 0, "junction descendants must never become candidates");
        assert_eq!(report.archived, 0, "junction descendants must never be archived");
        assert!(outside_file.exists(), "outside junction target must remain untouched");
        assert!(
            !archive_path_for(&outside_file).exists(),
            "no archive may be published next to the outside target"
        );
        assert!(
            !archive_path_for(&junction.join("outside.jsonl")).exists(),
            "no archive may be published through the junction path"
        );

        fs::remove_dir(&junction).expect("junction cleanup must remove only the junction entry");
        assert!(outside_file.exists(), "junction cleanup must not remove its target");
    }
}
