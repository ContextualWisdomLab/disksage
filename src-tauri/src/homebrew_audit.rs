//! Read-only Homebrew stale / orphan software audit.
//!
//! This module inventories formula leaves and casks, gathers last-use and dependency evidence,
//! and classifies each package as `stale`, `orphan`, `in-use`, or `unknown`. Classification never
//! becomes `stale` from atime alone: reverse dependencies, running processes, repository
//! toolchain references, and missing/unreliable last-use evidence keep the package out of
//! `stale`. The audit never uninstalls software.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::{Command, Stdio};
use std::time::Duration;

pub const HOMEBREW_AUDIT_SCHEMA_KIND: &str = "disksage.homebrew-audit/v1";
pub const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 120_000;
pub const DEFAULT_STALE_AFTER_DAYS: u64 = 90;
pub const DEFAULT_MAX_REPO_FILE_BYTES: u64 = 1_048_576;
pub const DEFAULT_MAX_REPO_MATCHES_PER_PACKAGE: usize = 20;

const MAX_COMMAND_OUTPUT_BYTES: usize = 1_048_576;
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(25);

const TOOLCHAIN_FILE_NAMES: &[&str] = &[
    ".tool-versions",
    "mise.toml",
    ".mise.toml",
    "Brewfile",
    "Dockerfile",
    "dockerfile",
    "pyproject.toml",
    "package.json",
    "Cargo.toml",
    "go.mod",
    "Gemfile",
];

const TOOLCHAIN_SUFFIXES: &[&str] = &[".yml", ".yaml"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HomebrewPackageKind {
    Formula,
    Cask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HomebrewClassification {
    Stale,
    Orphan,
    InUse,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomebrewLastUseEvidence {
    pub method: String,
    pub observed_at_ms: Option<u64>,
    pub evidence_complete: bool,
    pub atime_unreliable: bool,
    pub paths_checked: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomebrewRepoReference {
    pub path: String,
    pub matched_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomebrewPackageAudit {
    pub name: String,
    pub kind: HomebrewPackageKind,
    pub installed_on_request: bool,
    pub is_leaf: bool,
    pub installed_version: Option<String>,
    pub installed_at_ms: Option<u64>,
    pub installed_bytes: Option<u64>,
    pub prefix: Option<String>,
    pub reverse_dependencies: Vec<String>,
    pub autoremove_candidate: bool,
    pub running_pids: Vec<u32>,
    pub last_use: HomebrewLastUseEvidence,
    pub repo_references: Vec<HomebrewRepoReference>,
    pub classification: HomebrewClassification,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomebrewAuditReport {
    pub schema_kind: String,
    pub schema_version: u32,
    pub generated_at_ms: u64,
    pub brew_path: String,
    pub brew_prefix: String,
    pub stale_after_days: u64,
    pub repository_roots: Vec<String>,
    pub command_timeout_ms: u64,
    pub evidence_complete: bool,
    pub issues: Vec<String>,
    pub classification_counts: BTreeMap<String, u64>,
    pub packages: Vec<HomebrewPackageAudit>,
    pub filesystem_mutation_executed: bool,
}

#[derive(Debug, Clone)]
pub struct HomebrewAuditOptions {
    pub stale_after_days: u64,
    pub command_timeout_ms: u64,
    pub repository_roots: Vec<PathBuf>,
    pub max_repo_file_bytes: u64,
    pub max_repo_matches_per_package: usize,
    pub name_filter: BTreeSet<String>,
}

impl Default for HomebrewAuditOptions {
    fn default() -> Self {
        Self {
            stale_after_days: DEFAULT_STALE_AFTER_DAYS,
            command_timeout_ms: DEFAULT_COMMAND_TIMEOUT_MS,
            repository_roots: Vec::new(),
            max_repo_file_bytes: DEFAULT_MAX_REPO_FILE_BYTES,
            max_repo_matches_per_package: DEFAULT_MAX_REPO_MATCHES_PER_PACKAGE,
            name_filter: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HomebrewPackageEvidence {
    pub name: String,
    pub kind: HomebrewPackageKind,
    pub installed_on_request: bool,
    pub is_leaf: bool,
    pub installed_version: Option<String>,
    pub installed_at_ms: Option<u64>,
    pub installed_bytes: Option<u64>,
    pub prefix: Option<String>,
    pub reverse_dependencies: Vec<String>,
    pub autoremove_candidate: bool,
    pub running_pids: Vec<u32>,
    pub last_use: HomebrewLastUseEvidence,
    pub repo_references: Vec<HomebrewRepoReference>,
    pub evidence_gaps: Vec<String>,
}

fn reason_code(reason: &str) -> &str {
    reason.split_once(':').map_or(reason, |(code, _)| code)
}

fn is_incomplete_evidence_reason(reason: &str) -> bool {
    let code = reason_code(reason);
    code.starts_with("brew-command-")
        || matches!(
            code,
            "last-use-evidence-missing"
                | "atime-unreliable"
                | "install-time-only-no-use-evidence"
                | "brew-evidence-incomplete"
                | "size-scan-incomplete"
                | "prefix-unavailable"
                | "brew-uses-failed"
                | "active-use-probe-failed"
                | "active-use-timeout"
        )
}

fn active_use_probe_error(error: String) -> String {
    let code = if reason_code(&error) == "brew-command-timeout" {
        "active-use-timeout"
    } else {
        "active-use-probe-failed"
    };
    format!("{code}:{error}")
}

/// Pure classifier: never returns `Stale` from last-use age alone.
pub fn classify_package(
    evidence: &HomebrewPackageEvidence,
    now_ms: u64,
    stale_after_days: u64,
) -> (HomebrewClassification, Vec<String>) {
    let mut reasons = Vec::new();

    if !evidence.running_pids.is_empty() {
        reasons.push("running-process-from-prefix".into());
    }
    if !evidence.repo_references.is_empty() {
        reasons.push("referenced-by-repository-toolchain".into());
    }
    if !evidence.reverse_dependencies.is_empty() {
        reasons.push("has-reverse-dependencies".into());
    }
    if evidence.autoremove_candidate {
        reasons.push("autoremove-orphan-candidate".into());
    }
    for gap in &evidence.evidence_gaps {
        reasons.push(gap.clone());
    }
    if !evidence.last_use.evidence_complete {
        reasons.push("last-use-evidence-missing".into());
    }
    if evidence.last_use.atime_unreliable {
        reasons.push("atime-unreliable".into());
    }

    let stale_after_ms = stale_after_days.saturating_mul(86_400_000);
    match evidence.last_use.observed_at_ms {
        Some(observed) if now_ms.saturating_sub(observed) < stale_after_ms => {
            reasons.push("last-use-within-threshold".into());
        }
        Some(observed) if now_ms.saturating_sub(observed) >= stale_after_ms => {
            reasons.push("last-use-exceeds-threshold".into());
        }
        None => {
            if evidence.installed_at_ms.is_some() {
                reasons.push("install-time-only-no-use-evidence".into());
            }
        }
        _ => {}
    }

    reasons.sort();
    reasons.dedup();

    if reasons.iter().any(|r| r == "running-process-from-prefix")
        || reasons
            .iter()
            .any(|r| r == "referenced-by-repository-toolchain")
        || reasons.iter().any(|r| r == "has-reverse-dependencies")
        || reasons.iter().any(|r| r == "last-use-within-threshold")
    {
        return (HomebrewClassification::InUse, reasons);
    }

    if reasons
        .iter()
        .any(|r| r == "autoremove-orphan-candidate")
        && !evidence.is_leaf
        && evidence.kind == HomebrewPackageKind::Formula
    {
        return (HomebrewClassification::Orphan, reasons);
    }

    let blocking_unknown = reasons
        .iter()
        .any(|reason| is_incomplete_evidence_reason(reason));
    if blocking_unknown {
        return (HomebrewClassification::Unknown, reasons);
    }

    if reasons
        .iter()
        .any(|r| r == "last-use-exceeds-threshold")
        && (evidence.is_leaf || evidence.kind == HomebrewPackageKind::Cask)
        && evidence.reverse_dependencies.is_empty()
        && evidence.running_pids.is_empty()
        && evidence.repo_references.is_empty()
        && evidence.last_use.evidence_complete
        && !evidence.last_use.atime_unreliable
    {
        return (HomebrewClassification::Stale, reasons);
    }

    (HomebrewClassification::Unknown, reasons)
}

fn classification_key(value: HomebrewClassification) -> &'static str {
    match value {
        HomebrewClassification::Stale => "stale",
        HomebrewClassification::Orphan => "orphan",
        HomebrewClassification::InUse => "in-use",
        HomebrewClassification::Unknown => "unknown",
    }
}

#[cfg(unix)]
fn run_bounded_command(
    program: &Path,
    args: &[&str],
    timeout_ms: u64,
    max_output_bytes: usize,
) -> Result<(i32, String, String), String> {
    use crate::unix_process_group::{
        signal_private_process_group, spawn_bounded_cancellable_pipe_reader,
        wait_for_child_without_reap, NoReapWaitOutcome, PipeReaderCancellation,
    };
    use std::os::unix::process::CommandExt;

    let mut command = Command::new(program);
    command
        .args(args)
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
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
        .map_err(|error| format!("brew-command-spawn-failed:{error}"))?;
    let child_pid = child.id();
    let Some(stdout) = child.stdout.take() else {
        let _ = signal_private_process_group(child_pid, libc::SIGKILL);
        let _ = child.kill();
        let _ = child.wait();
        return Err("brew-command-stdout-unavailable".into());
    };
    let Some(stderr) = child.stderr.take() else {
        let _ = signal_private_process_group(child_pid, libc::SIGKILL);
        let _ = child.kill();
        let _ = child.wait();
        return Err("brew-command-stderr-unavailable".into());
    };
    let cancellation = PipeReaderCancellation::new();
    let stdout_reader = match spawn_bounded_cancellable_pipe_reader(
        stdout,
        max_output_bytes,
        COMMAND_POLL_INTERVAL,
        cancellation.clone(),
    ) {
        Ok(reader) => reader,
        Err(error) => {
            let _ = signal_private_process_group(child_pid, libc::SIGKILL);
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("brew-command-output-reader-failed:{error}"));
        }
    };
    let stderr_reader = match spawn_bounded_cancellable_pipe_reader(
        stderr,
        max_output_bytes,
        COMMAND_POLL_INTERVAL,
        cancellation.clone(),
    ) {
        Ok(reader) => reader,
        Err(error) => {
            let _ = signal_private_process_group(child_pid, libc::SIGKILL);
            let _ = child.kill();
            let _ = child.wait();
            cancellation.cancel();
            let _ = stdout_reader.join();
            return Err(format!("brew-command-output-reader-failed:{error}"));
        }
    };

    let status = match wait_for_child_without_reap(
        child_pid,
        Duration::from_millis(timeout_ms.max(1)),
        COMMAND_POLL_INTERVAL,
    ) {
        Ok(NoReapWaitOutcome::ExitedUnreaped) => {
            // The leader remains waitable here, so its PGID cannot be recycled while any
            // descendants retaining the output pipes are terminated.
            let _ = signal_private_process_group(child_pid, libc::SIGKILL);
            match child.wait() {
                Ok(status) => status,
                Err(error) => {
                    cancellation.cancel();
                    let _ = stdout_reader.join();
                    let _ = stderr_reader.join();
                    return Err(format!("brew-command-wait-failed:{error}"));
                }
            }
        }
        Ok(NoReapWaitOutcome::TimedOutStillRunning) => {
            let _ = signal_private_process_group(child_pid, libc::SIGKILL);
            let _ = child.kill();
            let _ = child.wait();
            cancellation.cancel();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err("brew-command-timeout".into());
        }
        Err(error) => {
            // Without a pinned no-reap observation, avoid a negative-PID signal that could
            // target a recycled process group. Settle only the child handle we still own.
            let _ = child.kill();
            let _ = child.wait();
            cancellation.cancel();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(format!("brew-command-wait-failed:{error}"));
        }
    };

    cancellation.cancel();
    let (stdout, stdout_truncated) = stdout_reader
        .join()
        .map_err(|_| "brew-command-output-reader-panicked".to_string())?
        .map_err(|error| format!("brew-command-output-read-failed:{error}"))?;
    let (stderr, stderr_truncated) = stderr_reader
        .join()
        .map_err(|_| "brew-command-output-reader-panicked".to_string())?
        .map_err(|error| format!("brew-command-output-read-failed:{error}"))?;
    if stdout_truncated || stderr_truncated {
        return Err("brew-command-output-too-large".into());
    }

    Ok((
        status.code().unwrap_or(1),
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    ))
}

#[cfg(not(unix))]
fn run_bounded_command(
    _program: &Path,
    _args: &[&str],
    _timeout_ms: u64,
    _max_output_bytes: usize,
) -> Result<(i32, String, String), String> {
    Err("homebrew-audit-unsupported-platform".into())
}

fn resolve_brew_path() -> Result<PathBuf, String> {
    for candidate in [
        Path::new("/opt/homebrew/bin/brew"),
        Path::new("/usr/local/bin/brew"),
        Path::new("/home/linuxbrew/.linuxbrew/bin/brew"),
    ] {
        if candidate.is_file() {
            return Ok(candidate.to_path_buf());
        }
    }
    Err("homebrew-brew-not-found".into())
}

fn lines_nonempty(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn directory_bytes(path: &Path, timeout_ms: u64) -> Result<u64, String> {
    if !path.exists() {
        return Err("prefix-unavailable".into());
    }
    let (code, out, err) = run_bounded_command(
        Path::new("/usr/bin/du"),
        &["-sk", &path.to_string_lossy()],
        timeout_ms.min(60_000).max(5_000),
        MAX_COMMAND_OUTPUT_BYTES,
    )?;
    if code != 0 {
        return Err(format!("size-scan-incomplete:{err}"));
    }
    let kb = out
        .split_whitespace()
        .next()
        .ok_or_else(|| "size-scan-incomplete".to_string())?
        .parse::<u64>()
        .map_err(|_| "size-scan-incomplete".to_string())?;
    Ok(kb.saturating_mul(1024))
}

fn formula_last_use(prefix: &Path, atime_unreliable_volume: bool) -> HomebrewLastUseEvidence {
    let mut paths_checked = Vec::new();
    let mut notes = Vec::new();
    let mut latest: Option<u64> = None;
    let mut complete = false;

    for sub in ["bin", "sbin"] {
        let dir = prefix.join(sub);
        if !dir.is_dir() {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => {
                notes.push(format!("unreadable-bin-dir:{}", dir.display()));
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            paths_checked.push(path.display().to_string());
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if let Ok(meta) = entry.metadata() {
                    let atime_ms = (meta.atime() as u64).saturating_mul(1000);
                    latest = Some(latest.map_or(atime_ms, |cur| cur.max(atime_ms)));
                    complete = true;
                    let mtime_ms = (meta.mtime() as u64).saturating_mul(1000);
                    if atime_ms == mtime_ms {
                        notes.push(format!("atime-equals-mtime:{}", path.display()));
                    }
                }
            }
            #[cfg(not(unix))]
            {
                notes.push("atime-unsupported-platform".into());
            }
        }
    }

    if paths_checked.is_empty() {
        notes.push("no-formula-executables-under-bin-sbin".into());
    }

    HomebrewLastUseEvidence {
        method: "formula-executable-atime".into(),
        observed_at_ms: latest,
        evidence_complete: complete && !paths_checked.is_empty(),
        atime_unreliable: atime_unreliable_volume
            || notes.iter().any(|n| n.starts_with("atime-equals-mtime:")),
        paths_checked,
        notes,
    }
}

fn cask_last_use_with_mdls(
    app_paths: &[PathBuf],
    mdls_path: &Path,
    timeout_ms: u64,
    max_output_bytes: usize,
) -> HomebrewLastUseEvidence {
    let mut paths_checked = Vec::new();
    let mut notes = Vec::new();
    let mut latest: Option<u64> = None;
    let mut complete = false;
    let mut probe_incomplete = false;

    for app in app_paths {
        paths_checked.push(app.display().to_string());
        if !app.exists() {
            notes.push(format!("app-missing:{}", app.display()));
            probe_incomplete = true;
            continue;
        }
        let app_arg = app.to_string_lossy();
        let output = run_bounded_command(
            mdls_path,
            &["-name", "kMDItemLastUsedDate", "-raw", &app_arg],
            timeout_ms,
            max_output_bytes,
        );
        match output {
            Ok((0, stdout, _)) => {
                let text = stdout.trim().to_string();
                if text.is_empty() || text == "(null)" {
                    notes.push(format!("spotlight-last-used-null:{}", app.display()));
                    probe_incomplete = true;
                } else if let Some(ms) = parse_mdls_date_to_ms(&text) {
                    latest = Some(latest.map_or(ms, |cur| cur.max(ms)));
                    complete = true;
                } else {
                    notes.push(format!("spotlight-last-used-unparsed:{text}"));
                    probe_incomplete = true;
                }
            }
            Ok((code, _, stderr)) => {
                notes.push(format!(
                    "spotlight-query-failed:{}:exit-status:{code}:stderr:{stderr}",
                    app.display()
                ));
                probe_incomplete = true;
            }
            Err(error) => {
                notes.push(format!("spotlight-query-failed:{}:{error}", app.display()));
                probe_incomplete = true;
            }
        }
    }

    if app_paths.is_empty() {
        notes.push("cask-app-path-unavailable".into());
    }

    HomebrewLastUseEvidence {
        method: "cask-app-spotlight-last-used".into(),
        observed_at_ms: latest,
        evidence_complete: complete && !probe_incomplete,
        atime_unreliable: false,
        paths_checked,
        notes,
    }
}

fn cask_last_use(app_paths: &[PathBuf], timeout_ms: u64) -> HomebrewLastUseEvidence {
    cask_last_use_with_mdls(
        app_paths,
        Path::new("/usr/bin/mdls"),
        timeout_ms,
        MAX_COMMAND_OUTPUT_BYTES,
    )
}

fn parse_mdls_date_to_ms(text: &str) -> Option<u64> {
    // mdls -raw typically yields "2024-01-02 03:04:05 +0000"
    let parsed = chrono_lite_parse(text)?;
    Some(parsed)
}

/// Minimal RFC-ish date parse without adding a chrono dependency.
fn chrono_lite_parse(text: &str) -> Option<u64> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let date = parts[0];
    let time = parts[1];
    let mut d = date.split('-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;
    let mut t = time.split(':');
    let hour: u32 = t.next()?.parse().ok()?;
    let minute: u32 = t.next()?.parse().ok()?;
    let second: u32 = t.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Approximate UTC epoch seconds using civil_from_days / days_from_civil style math.
    let days = days_from_civil(year, month as i32, day as i32);
    let secs = i64::from(days) * 86_400
        + i64::from(hour) * 3_600
        + i64::from(minute) * 60
        + i64::from(second);
    if secs < 0 {
        return None;
    }
    Some((secs as u64).saturating_mul(1000))
}

fn days_from_civil(year: i64, month: i32, day: i32) -> i32 {
    let mut y = year;
    let m = month;
    if m <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let mp = if m > 2 { m - 3 } else { m + 9 } as u32;
    let doy = (153 * mp + 2) / 5 + day as u32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe as i64 - 719_468) as i32
}

/// Unix volumes may mount with `noatime`; probe via `mount(8)`.
/// Non-unix platforms have no portable `mount`/`atime` contract — fail closed
/// (treat last-use atime as unreliable) so classification cannot become `stale`
/// from atime alone. Matches `atime-unsupported-platform` in formula_last_use.
#[cfg(unix)]
fn volume_atime_unreliable(prefix: &Path) -> bool {
    let output = Command::new("mount").output().ok();
    let Some(output) = output else {
        return true;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let prefix_str = prefix.to_string_lossy();
    for line in text.lines() {
        if line.contains("noatime") && (prefix_str.starts_with('/') && line.contains("on /")) {
            // Prefer matching the mounted root that contains the prefix.
            if let Some(on_idx) = line.find(" on ") {
                let rest = &line[on_idx + 4..];
                let mount_point = rest.split_whitespace().next().unwrap_or("");
                if mount_point != "/" && prefix_str.starts_with(mount_point) {
                    return true;
                }
                if mount_point == "/" {
                    // Data volume on macOS is often separate; treat system-root noatime carefully.
                    continue;
                }
            }
        }
    }
    false
}

#[cfg(not(unix))]
fn volume_atime_unreliable(_prefix: &Path) -> bool {
    true
}

fn classify_lsof_result(exit_code: i32, stdout: &str, stderr: &str) -> Result<Vec<u32>, String> {
    // lsof documents exit status 1 with no output as "no files were found". Every other
    // non-zero outcome leaves active-use evidence incomplete.
    if exit_code == 1 && stdout.is_empty() && stderr.is_empty() {
        return Ok(Vec::new());
    }
    if exit_code != 0 {
        return Err(format!(
            "active-use-probe-failed:lsof-exit-status:{exit_code}:stderr:{stderr}"
        ));
    }

    let mut pids = BTreeSet::new();
    for token in stdout.split(|c| c == '\n' || c == '\0') {
        let token = token.trim();
        if let Some(pid) = token.strip_prefix('p') {
            if let Ok(value) = pid.parse::<u32>() {
                pids.insert(value);
            }
        }
    }
    Ok(pids.into_iter().collect())
}

fn running_pids_under_prefix(prefix: &Path, timeout_ms: u64) -> Result<Vec<u32>, String> {
    if !prefix.exists() {
        return Ok(Vec::new());
    }
    let (code, out, err) = match run_bounded_command(
        Path::new("/usr/sbin/lsof"),
        &["-Fpc", "+D", &prefix.to_string_lossy()],
        timeout_ms.min(30_000).max(5_000),
        MAX_COMMAND_OUTPUT_BYTES,
    ) {
        Ok(value) => value,
        Err(error) => return Err(active_use_probe_error(error)),
    };
    classify_lsof_result(code, &out, &err)
}

fn record_running_pids(
    result: Result<Vec<u32>, String>,
    running_pids: &mut BTreeSet<u32>,
    evidence_gaps: &mut Vec<String>,
) {
    match result {
        Ok(pids) => running_pids.extend(pids),
        Err(error) => evidence_gaps.push(error),
    }
}

fn toolchain_tokens_for(name: &str) -> Vec<String> {
    let mut tokens = BTreeSet::new();
    tokens.insert(name.to_string());
    if let Some((base, _)) = name.split_once('@') {
        tokens.insert(base.to_string());
    }
    tokens.into_iter().collect()
}

fn scan_repo_references(
    roots: &[PathBuf],
    package_names: &BTreeSet<String>,
    max_file_bytes: u64,
    max_matches: usize,
) -> BTreeMap<String, Vec<HomebrewRepoReference>> {
    let mut packages_by_token: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut references: BTreeMap<String, Vec<HomebrewRepoReference>> = package_names
        .iter()
        .map(|name| (name.clone(), Vec::new()))
        .collect();
    for package_name in package_names {
        for token in toolchain_tokens_for(package_name) {
            packages_by_token
                .entry(token)
                .or_default()
                .push(package_name.clone());
        }
    }
    if max_matches == 0 {
        return references;
    }

    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let walker = walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                !matches!(
                    name.as_ref(),
                    ".git" | "node_modules" | "target" | ".venv" | "venv" | "__pycache__"
                        | ".Trash" | "Library"
                )
            });
        for entry in walker {
            let entry = match entry {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            let is_named = TOOLCHAIN_FILE_NAMES.iter().any(|candidate| *candidate == name);
            let is_ci = name.starts_with('.') == false
                && TOOLCHAIN_SUFFIXES
                    .iter()
                    .any(|suffix| name.ends_with(suffix))
                && entry
                    .path()
                    .components()
                    .any(|component| matches!(component.as_os_str().to_str(), Some(".github" | "ci" | ".circleci")));
            if !is_named && !is_ci && name != "Brewfile.lock.json" {
                continue;
            }
            let meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            if meta.len() > max_file_bytes {
                continue;
            }
            let content = match std::fs::read_to_string(entry.path()) {
                Ok(content) => content,
                Err(_) => continue,
            };
            let mut matched_packages = BTreeSet::new();
            for (token, package_names) in &packages_by_token {
                if !content.contains(token) {
                    continue;
                }
                for package_name in package_names {
                    if matched_packages.contains(package_name) {
                        continue;
                    }
                    if let Some(package_references) = references.get_mut(package_name) {
                        if package_references.len() >= max_matches {
                            continue;
                        }
                        package_references.push(HomebrewRepoReference {
                            path: entry.path().display().to_string(),
                            matched_token: token.clone(),
                        });
                        matched_packages.insert(package_name.clone());
                    }
                }
            }
        }
    }
    references
}

fn parse_info_json(
    payload: &str,
    formula_names: &BTreeSet<String>,
    cask_names: &BTreeSet<String>,
) -> Result<(Vec<serde_json::Value>, Vec<serde_json::Value>), String> {
    let value: serde_json::Value =
        serde_json::from_str(payload).map_err(|_| "brew-info-json-parse-failed".to_string())?;
    let formulae = value
        .get("formulae")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|item| {
            item.get("name")
                .and_then(|n| n.as_str())
                .is_some_and(|name| formula_names.contains(name))
        })
        .collect();
    let casks = value
        .get("casks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|item| {
            item.get("token")
                .or_else(|| item.get("name").and_then(|n| n.as_array()).and_then(|arr| arr.first()))
                .map(|token| {
                    if let Some(s) = token.as_str() {
                        cask_names.contains(s)
                    } else {
                        false
                    }
                })
                .unwrap_or(false)
                || item
                    .get("token")
                    .and_then(|t| t.as_str())
                    .is_some_and(|token| cask_names.contains(token))
        })
        .collect();
    Ok((formulae, casks))
}

fn cask_token(item: &serde_json::Value) -> Option<String> {
    item.get("token")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn cask_app_paths(item: &serde_json::Value, brew_prefix: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(artifacts) = item.get("artifacts").and_then(|v| v.as_array()) {
        for artifact in artifacts {
            if let Some(app) = artifact.get("app").and_then(|v| v.as_array()) {
                for entry in app {
                    if let Some(name) = entry.as_str() {
                        paths.push(PathBuf::from("/Applications").join(name));
                        if let Some(token) = cask_token(item) {
                            paths.push(
                                brew_prefix
                                    .join("Caskroom")
                                    .join(token)
                                    .join(name),
                            );
                        }
                    }
                }
            }
        }
    }
    paths
}

/// Run a complete read-only Homebrew audit against the local brew installation.
pub fn audit_homebrew(
    options: HomebrewAuditOptions,
    now_ms: u64,
) -> Result<HomebrewAuditReport, String> {
    let brew_path = resolve_brew_path()?;
    let mut issues = Vec::new();

    let (code, prefix_out, prefix_err) = run_bounded_command(
        &brew_path,
        &["--prefix"],
        options.command_timeout_ms,
        MAX_COMMAND_OUTPUT_BYTES,
    )?;
    if code != 0 {
        return Err(format!("brew-prefix-failed:{prefix_err}"));
    }
    let brew_prefix = PathBuf::from(prefix_out.trim());
    let atime_unreliable_volume = volume_atime_unreliable(&brew_prefix);

    let (code, leaves_out, leaves_err) = run_bounded_command(
        &brew_path,
        &["leaves", "-r"],
        options.command_timeout_ms,
        MAX_COMMAND_OUTPUT_BYTES,
    )?;
    if code != 0 {
        return Err(format!("brew-leaves-failed:{leaves_err}"));
    }
    let mut leaf_names: BTreeSet<String> = lines_nonempty(&leaves_out).into_iter().collect();

    let (code, cask_out, cask_err) = run_bounded_command(
        &brew_path,
        &["list", "--cask"],
        options.command_timeout_ms,
        MAX_COMMAND_OUTPUT_BYTES,
    )?;
    if code != 0 {
        return Err(format!("brew-list-cask-failed:{cask_err}"));
    }
    let mut cask_names: BTreeSet<String> = lines_nonempty(&cask_out).into_iter().collect();

    let (code, auto_out, auto_err) = run_bounded_command(
        &brew_path,
        &["autoremove", "--dry-run"],
        options.command_timeout_ms,
        MAX_COMMAND_OUTPUT_BYTES,
    )?;
    if code != 0 {
        issues.push(format!("brew-autoremove-dry-run-failed:{auto_err}"));
    }
    let autoremove_names: BTreeSet<String> = auto_out
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            // Typical: "Would uninstall formulae:\n  foo\n  bar"
            if line.is_empty()
                || line.starts_with("Would ")
                || line.starts_with("==>")
                || line.starts_with("Warning")
            {
                None
            } else {
                Some(line.trim_start_matches('-').trim().to_string())
            }
        })
        .filter(|name| !name.is_empty())
        .collect();

    if !options.name_filter.is_empty() {
        leaf_names = leaf_names
            .intersection(&options.name_filter)
            .cloned()
            .collect();
        cask_names = cask_names
            .intersection(&options.name_filter)
            .cloned()
            .collect();
    }

    let mut info_args = vec!["info".to_string(), "--json=v2".to_string()];
    let mut info_targets = Vec::new();
    info_targets.extend(leaf_names.iter().cloned());
    info_targets.extend(cask_names.iter().cloned());
    // Also include autoremove names that may not be leaves so orphan evidence is complete.
    for name in &autoremove_names {
        if options.name_filter.is_empty() || options.name_filter.contains(name) {
            info_targets.push(name.clone());
        }
    }
    info_targets.sort();
    info_targets.dedup();
    if info_targets.is_empty() {
        return Ok(HomebrewAuditReport {
            schema_kind: HOMEBREW_AUDIT_SCHEMA_KIND.into(),
            schema_version: 1,
            generated_at_ms: now_ms,
            brew_path: brew_path.display().to_string(),
            brew_prefix: brew_prefix.display().to_string(),
            stale_after_days: options.stale_after_days,
            repository_roots: options
                .repository_roots
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            command_timeout_ms: options.command_timeout_ms,
            evidence_complete: issues.is_empty(),
            issues,
            classification_counts: BTreeMap::new(),
            packages: Vec::new(),
            filesystem_mutation_executed: false,
        });
    }
    info_args.extend(info_targets.iter().cloned());
    let info_arg_refs: Vec<&str> = info_args.iter().map(String::as_str).collect();
    let (code, info_out, info_err) = run_bounded_command(
        &brew_path,
        &info_arg_refs,
        options.command_timeout_ms,
        MAX_COMMAND_OUTPUT_BYTES,
    )?;
    if code != 0 {
        return Err(format!("brew-info-failed:{info_err}"));
    }

    let formula_filter: BTreeSet<String> = info_targets.iter().cloned().collect();
    let (formulae, casks) = parse_info_json(&info_out, &formula_filter, &cask_names)?;
    let repo_package_names: BTreeSet<String> = formulae
        .iter()
        .filter_map(|item| item.get("name").and_then(|value| value.as_str()))
        .chain(
            casks
                .iter()
                .filter_map(|item| item.get("token").and_then(|value| value.as_str())),
        )
        .map(str::to_string)
        .collect();
    let repo_references_by_package = scan_repo_references(
        &options.repository_roots,
        &repo_package_names,
        options.max_repo_file_bytes,
        options.max_repo_matches_per_package,
    );

    let mut packages = Vec::new();

    for item in formulae {
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        if !options.name_filter.is_empty() && !options.name_filter.contains(&name) {
            continue;
        }
        let installed = item
            .get("installed")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first());
        let installed_version = installed
            .and_then(|v| v.get("version"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let installed_at_ms = installed
            .and_then(|v| v.get("time"))
            .and_then(|v| v.as_u64())
            .map(|secs| secs.saturating_mul(1000));
        let installed_on_request = installed
            .and_then(|v| v.get("installed_on_request"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let version = installed_version.clone().unwrap_or_default();
        let prefix = if version.is_empty() {
            brew_prefix.join("opt").join(&name)
        } else {
            brew_prefix.join("Cellar").join(&name).join(&version)
        };
        let opt_prefix = brew_prefix.join("opt").join(&name);

        let mut evidence_gaps = Vec::new();
        let installed_bytes = match directory_bytes(&prefix, options.command_timeout_ms)
            .or_else(|_| directory_bytes(&opt_prefix, options.command_timeout_ms))
        {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                evidence_gaps.push(error);
                None
            }
        };

        let (uses_code, uses_out, uses_err) = run_bounded_command(
            &brew_path,
            &["uses", "--installed", &name],
            options.command_timeout_ms,
            MAX_COMMAND_OUTPUT_BYTES,
        )?;
        let reverse_dependencies = if uses_code == 0 {
            lines_nonempty(&uses_out)
        } else {
            evidence_gaps.push(format!("brew-uses-failed:{uses_err}"));
            Vec::new()
        };

        let mut running_pid_set = BTreeSet::new();
        record_running_pids(
            running_pids_under_prefix(&opt_prefix, options.command_timeout_ms),
            &mut running_pid_set,
            &mut evidence_gaps,
        );
        let running_pids = running_pid_set
            .into_iter()
            .collect::<Vec<_>>();

        let last_use = formula_last_use(&opt_prefix, atime_unreliable_volume);
        let repo_references = repo_references_by_package
            .get(&name)
            .cloned()
            .unwrap_or_default();

        let evidence = HomebrewPackageEvidence {
            name: name.clone(),
            kind: HomebrewPackageKind::Formula,
            installed_on_request,
            is_leaf: leaf_names.contains(&name),
            installed_version,
            installed_at_ms,
            installed_bytes,
            prefix: Some(opt_prefix.display().to_string()),
            reverse_dependencies,
            autoremove_candidate: autoremove_names.contains(&name),
            running_pids,
            last_use,
            repo_references,
            evidence_gaps,
        };
        let (classification, reason_codes) =
            classify_package(&evidence, now_ms, options.stale_after_days);
        packages.push(HomebrewPackageAudit {
            name: evidence.name,
            kind: evidence.kind,
            installed_on_request: evidence.installed_on_request,
            is_leaf: evidence.is_leaf,
            installed_version: evidence.installed_version,
            installed_at_ms: evidence.installed_at_ms,
            installed_bytes: evidence.installed_bytes,
            prefix: evidence.prefix,
            reverse_dependencies: evidence.reverse_dependencies,
            autoremove_candidate: evidence.autoremove_candidate,
            running_pids: evidence.running_pids,
            last_use: evidence.last_use,
            repo_references: evidence.repo_references,
            classification,
            reason_codes,
        });
    }

    for item in casks {
        let name = match cask_token(&item) {
            Some(name) => name,
            None => continue,
        };
        if !options.name_filter.is_empty() && !options.name_filter.contains(&name) {
            continue;
        }
        let installed_version = item
            .get("installed")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let installed_at_ms = item
            .get("installed_time")
            .and_then(|v| v.as_u64())
            .map(|secs| secs.saturating_mul(1000));
        let prefix = brew_prefix.join("Caskroom").join(&name);
        let mut evidence_gaps = Vec::new();
        let installed_bytes = match directory_bytes(&prefix, options.command_timeout_ms) {
            Ok(bytes) => Some(bytes),
            Err(error) => {
                evidence_gaps.push(error);
                None
            }
        };
        let app_paths = cask_app_paths(&item, &brew_prefix);
        let last_use = cask_last_use(&app_paths, options.command_timeout_ms);
        let mut running_pid_set = BTreeSet::new();
        for app in &app_paths {
            record_running_pids(
                running_pids_under_prefix(app, options.command_timeout_ms),
                &mut running_pid_set,
                &mut evidence_gaps,
            );
        }
        let running_pids = running_pid_set.into_iter().collect::<Vec<_>>();
        let repo_references = repo_references_by_package
            .get(&name)
            .cloned()
            .unwrap_or_default();

        let evidence = HomebrewPackageEvidence {
            name: name.clone(),
            kind: HomebrewPackageKind::Cask,
            installed_on_request: true,
            is_leaf: true,
            installed_version,
            installed_at_ms,
            installed_bytes,
            prefix: Some(prefix.display().to_string()),
            reverse_dependencies: Vec::new(),
            autoremove_candidate: false,
            running_pids,
            last_use,
            repo_references,
            evidence_gaps,
        };
        let (classification, reason_codes) =
            classify_package(&evidence, now_ms, options.stale_after_days);
        packages.push(HomebrewPackageAudit {
            name: evidence.name,
            kind: evidence.kind,
            installed_on_request: evidence.installed_on_request,
            is_leaf: evidence.is_leaf,
            installed_version: evidence.installed_version,
            installed_at_ms: evidence.installed_at_ms,
            installed_bytes: evidence.installed_bytes,
            prefix: evidence.prefix,
            reverse_dependencies: evidence.reverse_dependencies,
            autoremove_candidate: evidence.autoremove_candidate,
            running_pids: evidence.running_pids,
            last_use: evidence.last_use,
            repo_references: evidence.repo_references,
            classification,
            reason_codes,
        });
    }

    packages.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| {
        format!("{:?}", a.kind).cmp(&format!("{:?}", b.kind))
    }));

    let mut classification_counts = BTreeMap::new();
    for package in &packages {
        *classification_counts
            .entry(classification_key(package.classification).to_string())
            .or_insert(0) += 1;
    }

    let evidence_complete = issues.is_empty()
        && packages.iter().all(|package| {
            !package
                .reason_codes
                .iter()
                .any(|reason| is_incomplete_evidence_reason(reason))
        });

    Ok(HomebrewAuditReport {
        schema_kind: HOMEBREW_AUDIT_SCHEMA_KIND.into(),
        schema_version: 1,
        generated_at_ms: now_ms,
        brew_path: brew_path.display().to_string(),
        brew_prefix: brew_prefix.display().to_string(),
        stale_after_days: options.stale_after_days,
        repository_roots: options
            .repository_roots
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        command_timeout_ms: options.command_timeout_ms,
        evidence_complete,
        issues,
        classification_counts,
        packages,
        filesystem_mutation_executed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn write_executable(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;

        std::fs::write(path, body).expect("write executable fixture");
        let mut permissions = std::fs::metadata(path)
            .expect("stat executable fixture")
            .permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(path, permissions).expect("chmod executable fixture");
    }

    fn base_evidence() -> HomebrewPackageEvidence {
        HomebrewPackageEvidence {
            name: "jmeter".into(),
            kind: HomebrewPackageKind::Formula,
            installed_on_request: true,
            is_leaf: true,
            installed_version: Some("5.6.3".into()),
            installed_at_ms: Some(1_000_000),
            installed_bytes: Some(50_000_000),
            prefix: Some("/opt/homebrew/opt/jmeter".into()),
            reverse_dependencies: Vec::new(),
            autoremove_candidate: false,
            running_pids: Vec::new(),
            last_use: HomebrewLastUseEvidence {
                method: "formula-executable-atime".into(),
                observed_at_ms: Some(1_000_000),
                evidence_complete: true,
                atime_unreliable: false,
                paths_checked: vec!["/opt/homebrew/opt/jmeter/bin/jmeter".into()],
                notes: Vec::new(),
            },
            repo_references: Vec::new(),
            evidence_gaps: Vec::new(),
        }
    }

    #[test]
    fn stale_requires_reliable_exceeded_last_use_and_no_other_signals() {
        let now = 1_000_000 + 100 * 86_400_000;
        let (class, reasons) = classify_package(&base_evidence(), now, 90);
        assert_eq!(class, HomebrewClassification::Stale);
        assert!(reasons.iter().any(|r| r == "last-use-exceeds-threshold"));
    }

    #[test]
    fn atime_alone_with_unreliable_flag_is_unknown_not_stale() {
        let mut evidence = base_evidence();
        evidence.last_use.atime_unreliable = true;
        let now = 1_000_000 + 100 * 86_400_000;
        let (class, reasons) = classify_package(&evidence, now, 90);
        assert_eq!(class, HomebrewClassification::Unknown);
        assert!(reasons.iter().any(|r| r == "atime-unreliable"));
    }

    #[cfg(not(unix))]
    #[test]
    fn volume_atime_unreliable_non_unix_fail_closed() {
        // Windows/other non-unix: no mount(8); treat volume atime as unreliable.
        assert!(volume_atime_unreliable(Path::new(
            r"C:\ProgramData\Homebrew"
        )));
    }

    #[test]
    fn missing_last_use_is_unknown_even_when_install_is_old() {
        let mut evidence = base_evidence();
        evidence.last_use.observed_at_ms = None;
        evidence.last_use.evidence_complete = false;
        let now = 1_000_000 + 100 * 86_400_000;
        let (class, _) = classify_package(&evidence, now, 90);
        assert_eq!(class, HomebrewClassification::Unknown);
    }

    #[test]
    fn incomplete_evidence_reasons_match_generated_details() {
        for reason in [
            "last-use-evidence-missing",
            "atime-unreliable",
            "install-time-only-no-use-evidence",
            "brew-evidence-incomplete",
            "size-scan-incomplete:permission denied",
            "prefix-unavailable",
            "brew-uses-failed:brew unavailable",
            "active-use-probe-failed:brew-command-spawn-failed:permission denied",
            "active-use-timeout:brew-command-timeout",
            "brew-command-spawn-failed:permission denied",
            "brew-command-timeout",
            "brew-command-wait-failed:interrupted",
        ] {
            assert!(is_incomplete_evidence_reason(reason), "{reason}");
        }
        assert!(!is_incomplete_evidence_reason("last-use-exceeds-threshold"));
    }

    #[test]
    fn generated_evidence_gap_with_details_blocks_stale_classification() {
        let mut evidence = base_evidence();
        evidence
            .evidence_gaps
            .push("brew-uses-failed:brew unavailable".into());
        let now = 1_000_000 + 100 * 86_400_000;
        let (class, _) = classify_package(&evidence, now, 90);
        assert_eq!(class, HomebrewClassification::Unknown);
    }

    #[test]
    fn active_use_probe_errors_retain_details_and_block_stale_classification() {
        assert_eq!(
            active_use_probe_error("brew-command-spawn-failed:permission denied".into()),
            "active-use-probe-failed:brew-command-spawn-failed:permission denied"
        );
        assert_eq!(
            active_use_probe_error("brew-command-timeout".into()),
            "active-use-timeout:brew-command-timeout"
        );

        let now = 1_000_000 + 100 * 86_400_000;
        for gap in [
            "active-use-probe-failed:brew-command-spawn-failed:permission denied",
            "active-use-timeout:brew-command-timeout",
        ] {
            let mut evidence = base_evidence();
            evidence.evidence_gaps.push(gap.into());
            assert_eq!(
                classify_package(&evidence, now, 90).0,
                HomebrewClassification::Unknown,
                "{gap}"
            );
        }
    }

    #[test]
    fn lsof_exit_status_contract_only_accepts_documented_empty_no_match() {
        assert_eq!(classify_lsof_result(1, "", ""), Ok(Vec::new()));
        assert_eq!(
            classify_lsof_result(1, "", "lsof: permission denied"),
            Err("active-use-probe-failed:lsof-exit-status:1:stderr:lsof: permission denied".into())
        );
        assert_eq!(
            classify_lsof_result(2, "", ""),
            Err("active-use-probe-failed:lsof-exit-status:2:stderr:".into())
        );
    }

    #[test]
    fn running_pid_collection_preserves_probe_errors_as_evidence_gaps() {
        let mut running_pids = BTreeSet::new();
        let mut evidence_gaps = Vec::new();
        record_running_pids(Ok(vec![42, 42]), &mut running_pids, &mut evidence_gaps);
        record_running_pids(
            Err("active-use-probe-failed:brew-command-spawn-failed:permission denied".into()),
            &mut running_pids,
            &mut evidence_gaps,
        );

        assert_eq!(running_pids.into_iter().collect::<Vec<_>>(), vec![42]);
        assert_eq!(
            evidence_gaps,
            vec!["active-use-probe-failed:brew-command-spawn-failed:permission denied"]
        );
    }

    #[test]
    fn repo_reference_scan_indexes_packages_and_preserves_limits() {
        let repo = tempfile::tempdir().expect("temp repo");
        std::fs::write(repo.path().join("Brewfile"), "alpha beta@2").expect("write Brewfile");
        std::fs::write(repo.path().join("package.json"), "alpha beta@2")
            .expect("write package.json");
        std::fs::write(repo.path().join("mise.toml"), "gamma".repeat(20))
            .expect("write oversized mise.toml");

        let package_names = BTreeSet::from([
            "alpha".to_string(),
            "beta@2".to_string(),
            "gamma".to_string(),
        ]);
        let references = scan_repo_references(&[repo.path().to_path_buf()], &package_names, 32, 1);

        assert_eq!(references["alpha"].len(), 1);
        assert_eq!(references["beta@2"].len(), 1);
        assert_eq!(references["beta@2"][0].matched_token, "beta");
        assert!(references["gamma"].is_empty());
    }

    #[test]
    fn repo_reference_forces_in_use() {
        let mut evidence = base_evidence();
        evidence.repo_references.push(HomebrewRepoReference {
            path: "/tmp/repo/Brewfile".into(),
            matched_token: "jmeter".into(),
        });
        let now = 1_000_000 + 100 * 86_400_000;
        let (class, reasons) = classify_package(&evidence, now, 90);
        assert_eq!(class, HomebrewClassification::InUse);
        assert!(reasons
            .iter()
            .any(|r| r == "referenced-by-repository-toolchain"));
    }

    #[test]
    fn reverse_dependencies_force_in_use() {
        let mut evidence = base_evidence();
        evidence.reverse_dependencies.push("other".into());
        evidence.is_leaf = false;
        let now = 1_000_000 + 100 * 86_400_000;
        let (class, _) = classify_package(&evidence, now, 90);
        assert_eq!(class, HomebrewClassification::InUse);
    }

    #[test]
    fn autoremove_non_leaf_is_orphan() {
        let mut evidence = base_evidence();
        evidence.is_leaf = false;
        evidence.autoremove_candidate = true;
        evidence.last_use.observed_at_ms = None;
        evidence.last_use.evidence_complete = false;
        // orphan path checked before unknown blockers only when autoremove and not leaf;
        // missing last-use still present — classifier returns orphan when autoremove fires
        // before unknown if we ordered that way. Current order: in-use checks, then orphan,
        // then unknown. Missing last-use does not block orphan.
        let (class, reasons) = classify_package(&evidence, 2_000_000, 90);
        assert_eq!(class, HomebrewClassification::Orphan);
        assert!(reasons
            .iter()
            .any(|r| r == "autoremove-orphan-candidate"));
    }

    #[test]
    fn recent_last_use_is_in_use() {
        let mut evidence = base_evidence();
        evidence.last_use.observed_at_ms = Some(10_000_000);
        let (class, reasons) = classify_package(&evidence, 10_000_000 + 1_000, 90);
        assert_eq!(class, HomebrewClassification::InUse);
        assert!(reasons.iter().any(|r| r == "last-use-within-threshold"));
    }

    #[test]
    fn mdls_date_parser_accepts_common_spotlight_format() {
        let ms = parse_mdls_date_to_ms("2024-01-02 03:04:05 +0000").expect("parse");
        assert!(ms > 1_700_000_000_000);
    }

    #[cfg(unix)]
    #[test]
    fn cask_spotlight_probe_obeys_timeout_and_output_cap() {
        let fixture = tempfile::tempdir().expect("temp fixture");
        let app = fixture.path().join("Fixture.app");
        std::fs::create_dir(&app).expect("create app fixture");

        let slow_mdls = fixture.path().join("slow-mdls");
        write_executable(&slow_mdls, "#!/bin/sh\nsleep 30\n");
        let started = std::time::Instant::now();
        let timed_out = cask_last_use_with_mdls(&[app.clone()], &slow_mdls, 20, 64);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "Spotlight probe exceeded its bounded timeout"
        );
        assert!(!timed_out.evidence_complete);
        assert!(timed_out
            .notes
            .iter()
            .any(|note| note.contains("brew-command-timeout")));

        let noisy_mdls = fixture.path().join("noisy-mdls");
        write_executable(&noisy_mdls, "#!/bin/sh\nprintf '0123456789abcdef'\n");
        let over_cap = cask_last_use_with_mdls(&[app], &noisy_mdls, 1_000, 8);
        assert!(!over_cap.evidence_complete);
        assert!(over_cap
            .notes
            .iter()
            .any(|note| note.contains("brew-command-output-too-large")));
    }
}
