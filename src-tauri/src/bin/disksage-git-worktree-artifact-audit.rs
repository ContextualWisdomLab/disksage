//! Read-only generated-artifact audit for exact stale-worktree removal candidates.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use disksage_lib::git_worktree::{audit_git_worktrees, GitWorktreeAuditOptions};
use disksage_lib::git_worktree_artifact::{
    audit_git_worktree_artifacts, public_summary, GitWorktreeArtifactAuditOptions,
};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    repository_root: PathBuf,
    retention_references: Vec<String>,
    private_output: Option<PathBuf>,
    worktree_options: GitWorktreeAuditOptions,
    artifact_options: GitWorktreeArtifactAuditOptions,
}

fn usage() -> &'static str {
    "usage: disksage-git-worktree-artifact-audit --repository-root ABSOLUTE_PATH --reference-ref REF [--reference-ref REF ...] [--private-output NEW_ABSOLUTE_JSON_PATH] [--command-timeout-ms N] [--worktree-size-scan-timeout-ms N] [--max-worktrees N] [--max-entries-per-worktree N] [--max-active-pids N] [--artifact-discovery-timeout-ms N] [--max-artifact-discovery-entries N] [--artifact-size-scan-timeout-ms N] [--max-entries-per-artifact N] [--max-artifacts N]"
}

fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

fn parse_number<T: std::str::FromStr>(
    args: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<T, String> {
    value(args, index, flag)?
        .parse()
        .map_err(|_| format!("{flag}는 올바른 정수여야 함"))
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut repository_root = None;
    let mut retention_references = Vec::new();
    let mut private_output = None;
    let mut worktree_options = GitWorktreeAuditOptions::default();
    let mut artifact_options = GitWorktreeArtifactAuditOptions::default();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--repository-root" => {
                if repository_root.is_some() {
                    return Err("--repository-root는 한 번만 지정할 수 있음".into());
                }
                repository_root =
                    Some(PathBuf::from(value(args, &mut index, "--repository-root")?));
            }
            "--reference-ref" => {
                retention_references.push(value(args, &mut index, "--reference-ref")?)
            }
            "--private-output" => {
                if private_output.is_some() {
                    return Err("--private-output은 한 번만 지정할 수 있음".into());
                }
                private_output = Some(PathBuf::from(value(args, &mut index, "--private-output")?));
            }
            "--command-timeout-ms" => {
                worktree_options.command_timeout_ms =
                    parse_number(args, &mut index, "--command-timeout-ms")?;
            }
            "--worktree-size-scan-timeout-ms" => {
                worktree_options.size_scan_timeout_ms =
                    parse_number(args, &mut index, "--worktree-size-scan-timeout-ms")?;
            }
            "--max-worktrees" => {
                worktree_options.max_worktrees = parse_number(args, &mut index, "--max-worktrees")?;
            }
            "--max-entries-per-worktree" => {
                worktree_options.max_entries_per_worktree =
                    parse_number(args, &mut index, "--max-entries-per-worktree")?;
            }
            "--max-active-pids" => {
                worktree_options.max_active_pids =
                    parse_number(args, &mut index, "--max-active-pids")?;
            }
            "--artifact-discovery-timeout-ms" => {
                artifact_options.discovery_timeout_ms =
                    parse_number(args, &mut index, "--artifact-discovery-timeout-ms")?;
            }
            "--max-artifact-discovery-entries" => {
                artifact_options.max_discovery_entries_per_worktree =
                    parse_number(args, &mut index, "--max-artifact-discovery-entries")?;
            }
            "--artifact-size-scan-timeout-ms" => {
                artifact_options.size_scan_timeout_ms =
                    parse_number(args, &mut index, "--artifact-size-scan-timeout-ms")?;
            }
            "--max-entries-per-artifact" => {
                artifact_options.max_entries_per_artifact =
                    parse_number(args, &mut index, "--max-entries-per-artifact")?;
            }
            "--max-artifacts" => {
                artifact_options.max_artifacts = parse_number(args, &mut index, "--max-artifacts")?;
            }
            "--help" | "-h" => return Err(usage().into()),
            unknown => return Err(format!("알 수 없는 인자: {unknown}")),
        }
        index += 1;
    }
    let repository_root =
        repository_root.ok_or_else(|| "--repository-root 값이 필요함".to_string())?;
    if !repository_root.is_absolute() {
        return Err("--repository-root는 절대 경로여야 함".into());
    }
    if private_output
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err("--private-output은 절대 경로여야 함".into());
    }
    if retention_references.is_empty() {
        return Err("--reference-ref 값이 하나 이상 필요함".into());
    }
    Ok(Args {
        repository_root,
        retention_references,
        private_output,
        worktree_options,
        artifact_options,
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

fn write_new_private_json(path: &PathBuf, encoded: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options
        .open(path)
        .map_err(|_| "git-worktree-artifact-private-output-create-failed".to_string())?;
    output
        .write_all(encoded)
        .map_err(|_| "git-worktree-artifact-private-output-write-failed".to_string())?;
    output
        .sync_all()
        .map_err(|_| "git-worktree-artifact-private-output-sync-failed".to_string())
}

fn run_with_args(args: Args, observed_at_ms: u64) -> Result<serde_json::Value, String> {
    let worktree_audit = audit_git_worktrees(
        &args.repository_root,
        &args.retention_references,
        args.worktree_options,
        observed_at_ms,
    )?;
    let report =
        audit_git_worktree_artifacts(&worktree_audit, args.artifact_options, observed_at_ms)?;
    let mut summary =
        serde_json::to_value(public_summary(&report)).map_err(|error| error.to_string())?;
    if let Some(private_output) = &args.private_output {
        let encoded = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
        write_new_private_json(private_output, &encoded)?;
        let sha256: String = Sha256::digest(&encoded)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        summary
            .as_object_mut()
            .ok_or_else(|| "git-worktree-artifact-public-summary-not-object".to_string())?
            .insert(
                "private_report".into(),
                serde_json::json!({
                    "written": true,
                    "bytes": encoded.len(),
                    "sha256": sha256,
                    "unix_mode": "0600",
                    "create_new": true,
                    "contains_sensitive_local_paths": true,
                    "is_approval": false,
                    "is_execution": false
                }),
            );
    }
    Ok(summary)
}

fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let output = run_with_args(parse_args(&raw)?, now_ms())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage Git worktree artifact audit: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_requires_absolute_root_and_reference_and_defaults_read_only() {
        let args = parse_args(&[
            "--repository-root".into(),
            "/tmp/repository".into(),
            "--reference-ref".into(),
            "origin/develop".into(),
        ])
        .unwrap();
        assert_eq!(args.retention_references, vec!["origin/develop"]);
        assert_eq!(args.worktree_options, GitWorktreeAuditOptions::default());
        assert_eq!(
            args.artifact_options,
            GitWorktreeArtifactAuditOptions::default()
        );
        assert!(args.private_output.is_none());

        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&[
            "--repository-root".into(),
            "relative".into(),
            "--reference-ref".into(),
            "develop".into(),
        ])
        .is_err());
    }

    #[test]
    fn parser_accepts_all_bounded_options_and_rejects_duplicates() {
        let args = parse_args(&[
            "--repository-root".into(),
            "/tmp/repository".into(),
            "--reference-ref".into(),
            "develop".into(),
            "--private-output".into(),
            "/tmp/private.json".into(),
            "--command-timeout-ms".into(),
            "2000".into(),
            "--worktree-size-scan-timeout-ms".into(),
            "3000".into(),
            "--max-worktrees".into(),
            "10".into(),
            "--max-entries-per-worktree".into(),
            "1000".into(),
            "--max-active-pids".into(),
            "8".into(),
            "--artifact-discovery-timeout-ms".into(),
            "4000".into(),
            "--max-artifact-discovery-entries".into(),
            "2000".into(),
            "--artifact-size-scan-timeout-ms".into(),
            "5000".into(),
            "--max-entries-per-artifact".into(),
            "3000".into(),
            "--max-artifacts".into(),
            "20".into(),
        ])
        .unwrap();
        assert_eq!(args.worktree_options.command_timeout_ms, 2000);
        assert_eq!(args.artifact_options.discovery_timeout_ms, 4000);
        assert_eq!(args.artifact_options.max_artifacts, 20);

        assert!(parse_args(&[
            "--repository-root".into(),
            "/tmp/repository".into(),
            "--repository-root".into(),
            "/tmp/other".into(),
            "--reference-ref".into(),
            "develop".into(),
        ])
        .is_err());
    }

    #[test]
    fn private_output_is_create_new_and_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("audit.json");
        write_new_private_json(&output, b"{\"first\":true}").unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), b"{\"first\":true}");
        assert!(write_new_private_json(&output, b"{\"second\":true}").is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"{\"first\":true}");
    }
}
