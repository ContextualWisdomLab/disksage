//! Explicit, approval-bound stale Git worktree removal.

use disksage_lib::git_worktree::{
    audit_git_worktrees, public_summary as audit_public_summary, GitWorktreeAuditOptions,
};
use disksage_lib::git_worktree_removal::{
    create_git_worktree_removal_approval, execute_git_worktree_removal,
    public_summary as removal_public_summary,
};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    repository_root: PathBuf,
    retention_references: Vec<String>,
    confirm_plan_fingerprint: Option<String>,
    approval_phrase: Option<String>,
    approved_by: Option<String>,
    rationale: Option<String>,
    approved_at_ms: Option<u64>,
    journal: Option<PathBuf>,
    execute: bool,
    options: GitWorktreeAuditOptions,
}

fn usage() -> &'static str {
    "usage: disksage-git-worktree-remove --repository-root ABSOLUTE_PATH --reference-ref REF [--reference-ref REF ...] [--command-timeout-ms N] [--size-scan-timeout-ms N] [--max-worktrees N] [--max-entries-per-worktree N] [--max-active-pids N] [--execute --confirm-plan-fingerprint HEX --approval-phrase EXACT_PHRASE --approved-by human:IDENTITY --rationale TEXT --approved-at-ms N --journal NEW_ABSOLUTE_JSONL_PATH]"
}

fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

fn number<T: std::str::FromStr>(
    args: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<T, String> {
    value(args, index, flag)?
        .parse()
        .map_err(|_| format!("{flag}는 올바른 정수여야 함"))
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args {
        repository_root: PathBuf::new(),
        retention_references: Vec::new(),
        confirm_plan_fingerprint: None,
        approval_phrase: None,
        approved_by: None,
        rationale: None,
        approved_at_ms: None,
        journal: None,
        execute: false,
        options: GitWorktreeAuditOptions::default(),
    };
    let mut repository_root = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--repository-root" => {
                repository_root =
                    Some(PathBuf::from(value(args, &mut index, "--repository-root")?));
            }
            "--reference-ref" => {
                parsed
                    .retention_references
                    .push(value(args, &mut index, "--reference-ref")?)
            }
            "--confirm-plan-fingerprint" => {
                parsed.confirm_plan_fingerprint =
                    Some(value(args, &mut index, "--confirm-plan-fingerprint")?)
            }
            "--approval-phrase" => {
                parsed.approval_phrase = Some(value(args, &mut index, "--approval-phrase")?)
            }
            "--approved-by" => parsed.approved_by = Some(value(args, &mut index, "--approved-by")?),
            "--rationale" => parsed.rationale = Some(value(args, &mut index, "--rationale")?),
            "--approved-at-ms" => {
                parsed.approved_at_ms = Some(number(args, &mut index, "--approved-at-ms")?)
            }
            "--journal" => {
                parsed.journal = Some(PathBuf::from(value(args, &mut index, "--journal")?))
            }
            "--execute" => parsed.execute = true,
            "--command-timeout-ms" => {
                parsed.options.command_timeout_ms =
                    number(args, &mut index, "--command-timeout-ms")?
            }
            "--size-scan-timeout-ms" => {
                parsed.options.size_scan_timeout_ms =
                    number(args, &mut index, "--size-scan-timeout-ms")?
            }
            "--max-worktrees" => {
                parsed.options.max_worktrees = number(args, &mut index, "--max-worktrees")?
            }
            "--max-entries-per-worktree" => {
                parsed.options.max_entries_per_worktree =
                    number(args, &mut index, "--max-entries-per-worktree")?
            }
            "--max-active-pids" => {
                parsed.options.max_active_pids = number(args, &mut index, "--max-active-pids")?
            }
            "--help" | "-h" => return Err(usage().into()),
            unknown => return Err(format!("알 수 없는 인자: {unknown}")),
        }
        index += 1;
    }
    parsed.repository_root =
        repository_root.ok_or_else(|| "--repository-root 값이 필요함".to_string())?;
    if !parsed.repository_root.is_absolute() {
        return Err("--repository-root는 절대 경로여야 함".into());
    }
    if parsed.retention_references.is_empty() {
        return Err("--reference-ref 값이 하나 이상 필요함".into());
    }
    if parsed
        .journal
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err("--journal은 절대 경로여야 함".into());
    }

    let execution_fields_present = parsed.confirm_plan_fingerprint.is_some()
        || parsed.approval_phrase.is_some()
        || parsed.approved_by.is_some()
        || parsed.rationale.is_some()
        || parsed.approved_at_ms.is_some()
        || parsed.journal.is_some();
    if parsed.execute {
        if parsed.confirm_plan_fingerprint.is_none()
            || parsed.approval_phrase.is_none()
            || parsed.approved_by.is_none()
            || parsed.rationale.is_none()
            || parsed.approved_at_ms.is_none()
            || parsed.journal.is_none()
        {
            return Err("모든 실행 승인 인자가 필요함".into());
        }
    } else if execution_fields_present {
        return Err("실행 승인 인자는 --execute와 함께 사용해야 함".into());
    }
    Ok(parsed)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

fn run_with_args(args: Args, observed_at_ms: u64) -> Result<serde_json::Value, String> {
    let audit = audit_git_worktrees(
        &args.repository_root,
        &args.retention_references,
        args.options,
        observed_at_ms,
    )?;
    if !args.execute {
        return Ok(serde_json::json!({
            "mode": "read-only-audit",
            "execute_requested": false,
            "audit": audit_public_summary(&audit),
            "filesystem_mutation_executed": false,
        }));
    }

    let approval = create_git_worktree_removal_approval(
        &audit,
        args.approval_phrase
            .as_deref()
            .ok_or_else(|| "approval phrase missing".to_string())?,
        args.approved_by
            .as_deref()
            .ok_or_else(|| "approved-by missing".to_string())?,
        args.rationale
            .as_deref()
            .ok_or_else(|| "rationale missing".to_string())?,
        args.approved_at_ms
            .ok_or_else(|| "approved-at missing".to_string())?,
    )?;
    let report = execute_git_worktree_removal(
        &args.repository_root,
        &args.retention_references,
        args.options,
        &approval,
        args.confirm_plan_fingerprint
            .as_deref()
            .ok_or_else(|| "confirmed fingerprint missing".to_string())?,
        args.journal
            .as_deref()
            .ok_or_else(|| "journal missing".to_string())?,
        observed_at_ms,
    )?;
    Ok(serde_json::to_value(removal_public_summary(&report)).map_err(|error| error.to_string())?)
}

fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let value = run_with_args(parse_args(&raw)?, now_ms())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage Git worktree removal: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_args() -> Vec<String> {
        vec![
            "--repository-root".into(),
            "/tmp/repository".into(),
            "--reference-ref".into(),
            "origin/develop".into(),
        ]
    }

    #[test]
    fn defaults_to_read_only_audit() {
        let args = parse_args(&base_args()).unwrap();
        assert!(!args.execute);
        assert!(args.confirm_plan_fingerprint.is_none());
        assert!(args.journal.is_none());
        assert_eq!(
            args.options.command_timeout_ms,
            GitWorktreeAuditOptions::default().command_timeout_ms
        );
    }

    #[test]
    fn execute_requires_every_attributed_approval_field() {
        let mut incomplete = base_args();
        incomplete.push("--execute".into());
        assert!(parse_args(&incomplete).is_err());

        let mut complete = base_args();
        complete.extend([
            "--execute".into(),
            "--confirm-plan-fingerprint".into(),
            "a".repeat(64),
            "--approval-phrase".into(),
            format!("DiskSage stale worktree 1 4096 승인 {}", "a".repeat(64)),
            "--approved-by".into(),
            "human:test".into(),
            "--rationale".into(),
            "reviewed exact clean merged worktree".into(),
            "--approved-at-ms".into(),
            "100".into(),
            "--journal".into(),
            "/tmp/removal.jsonl".into(),
        ]);
        let parsed = parse_args(&complete).unwrap();
        assert!(parsed.execute);
        assert_eq!(parsed.approved_at_ms, Some(100));
        assert_eq!(parsed.journal, Some(PathBuf::from("/tmp/removal.jsonl")));
    }

    #[test]
    fn execution_fields_without_execute_and_relative_paths_are_rejected() {
        let mut with_approval = base_args();
        with_approval.extend(["--approved-by".into(), "human:test".into()]);
        assert!(parse_args(&with_approval).is_err());

        let mut relative_root = base_args();
        relative_root[1] = "relative".into();
        assert!(parse_args(&relative_root).is_err());

        let mut relative_journal = base_args();
        relative_journal.extend([
            "--execute".into(),
            "--confirm-plan-fingerprint".into(),
            "a".repeat(64),
            "--approval-phrase".into(),
            "phrase".into(),
            "--approved-by".into(),
            "human:test".into(),
            "--rationale".into(),
            "reviewed exact clean merged worktree".into(),
            "--approved-at-ms".into(),
            "100".into(),
            "--journal".into(),
            "relative.jsonl".into(),
        ]);
        assert!(parse_args(&relative_journal).is_err());
    }
}
