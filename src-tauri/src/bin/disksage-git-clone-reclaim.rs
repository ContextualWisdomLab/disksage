//! Headless exact-evidence planning and OS Trash execution for stale PR clones.

use disksage_lib::git_clone_reclaim::{
    approve_git_clone_reclaim, execute_git_clone_reclaim, plan_git_clone_reclaim,
};
use disksage_lib::git_worktree::{
    validate_reference, GitWorktreeAuditOptions, MAX_LOCAL_COMMAND_TIMEOUT_MS,
};
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: disksage-git-clone-reclaim --repository-root ABSOLUTE_PATH --reference-ref REF [--reference-ref REF ...] [--include-closed-pull-requests] [--stale-open-pull-request-cutoff-ms N] [--execute --plan-fingerprint HEX64 --confirm EXACT_PHRASE --approved-by HUMAN_ID --rationale TEXT --journal-path ABSOLUTE_PATH]";

#[derive(Debug, PartialEq, Eq)]
struct Args {
    repository_root: PathBuf,
    retention_references: Vec<String>,
    include_closed_pull_requests: bool,
    stale_open_cutoff_ms: Option<u64>,
    execution: Option<ExecutionArgs>,
}

#[derive(Debug, PartialEq, Eq)]
struct ExecutionArgs {
    plan_fingerprint: String,
    confirmation: String,
    approved_by: String,
    rationale: String,
    journal_path: PathBuf,
}

fn value(raw: &[OsString], index: &mut usize, flag: &str) -> Result<OsString, String> {
    *index += 1;
    raw.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag}-value-missing"))
}

fn text(raw: &[OsString], index: &mut usize, flag: &str) -> Result<String, String> {
    value(raw, index, flag)?
        .into_string()
        .map_err(|_| "git-clone-reclaim-invalid-argument-encoding".into())
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), String> {
    if slot.replace(value).is_some() {
        Err("git-clone-reclaim-duplicate-option".into())
    } else {
        Ok(())
    }
}

fn parse_args(raw: &[OsString]) -> Result<Option<Args>, String> {
    if raw.len() == 1 && matches!(raw[0].to_str(), Some("--help") | Some("-h")) {
        return Ok(None);
    }
    let mut root = None;
    let mut references = Vec::new();
    let mut include_closed_pull_requests = false;
    let mut cutoff = None;
    let mut execute = false;
    let mut fingerprint = None;
    let mut confirmation = None;
    let mut approved_by = None;
    let mut rationale = None;
    let mut journal = None;
    let mut index = 0;
    while index < raw.len() {
        let flag = raw[index]
            .to_str()
            .ok_or_else(|| "git-clone-reclaim-invalid-argument-encoding".to_string())?;
        match flag {
            "--repository-root" => {
                set_once(&mut root, PathBuf::from(value(raw, &mut index, flag)?))?
            }
            "--reference-ref" => {
                let reference = text(raw, &mut index, flag)?;
                validate_reference(&reference)?;
                references.push(reference);
            }
            "--include-closed-pull-requests" if !include_closed_pull_requests => {
                include_closed_pull_requests = true;
            }
            "--include-closed-pull-requests" => {
                return Err("git-clone-reclaim-duplicate-option".into())
            }
            "--stale-open-pull-request-cutoff-ms" => set_once(
                &mut cutoff,
                text(raw, &mut index, flag)?
                    .parse()
                    .map_err(|_| "git-clone-reclaim-cutoff-invalid".to_string())?,
            )?,
            "--execute" if !execute => execute = true,
            "--execute" => return Err("git-clone-reclaim-duplicate-option".into()),
            "--plan-fingerprint" => set_once(&mut fingerprint, text(raw, &mut index, flag)?)?,
            "--confirm" => set_once(&mut confirmation, text(raw, &mut index, flag)?)?,
            "--approved-by" => set_once(&mut approved_by, text(raw, &mut index, flag)?)?,
            "--rationale" => set_once(&mut rationale, text(raw, &mut index, flag)?)?,
            "--journal-path" => {
                set_once(&mut journal, PathBuf::from(value(raw, &mut index, flag)?))?
            }
            "--help" | "-h" => return Err("git-clone-reclaim-help-must-be-used-alone".into()),
            _ => return Err("git-clone-reclaim-unknown-argument".into()),
        }
        index += 1;
    }
    let repository_root = root.ok_or_else(|| "git-clone-reclaim-root-missing".to_string())?;
    if !repository_root.is_absolute() || references.is_empty() {
        return Err("git-clone-reclaim-plan-input-invalid".into());
    }
    let execution_values_present = fingerprint.is_some()
        || confirmation.is_some()
        || approved_by.is_some()
        || rationale.is_some()
        || journal.is_some();
    let execution = if execute {
        let journal_path =
            journal.ok_or_else(|| "git-clone-reclaim-execution-input-missing".to_string())?;
        if !journal_path.is_absolute() {
            return Err("git-clone-reclaim-journal-path-invalid".into());
        }
        Some(ExecutionArgs {
            plan_fingerprint: fingerprint
                .ok_or_else(|| "git-clone-reclaim-execution-input-missing".to_string())?,
            confirmation: confirmation
                .ok_or_else(|| "git-clone-reclaim-execution-input-missing".to_string())?,
            approved_by: approved_by
                .ok_or_else(|| "git-clone-reclaim-execution-input-missing".to_string())?,
            rationale: rationale
                .ok_or_else(|| "git-clone-reclaim-execution-input-missing".to_string())?,
            journal_path,
        })
    } else if execution_values_present {
        return Err("git-clone-reclaim-execution-flag-missing".into());
    } else {
        None
    };
    Ok(Some(Args {
        repository_root,
        retention_references: references,
        include_closed_pull_requests,
        stale_open_cutoff_ms: cutoff,
        execution,
    }))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

fn run(args: Args) -> Result<serde_json::Value, String> {
    let options = GitWorktreeAuditOptions {
        command_timeout_ms: MAX_LOCAL_COMMAND_TIMEOUT_MS,
        ..GitWorktreeAuditOptions::default()
    };
    let plan = plan_git_clone_reclaim(
        &args.repository_root,
        &args.retention_references,
        args.include_closed_pull_requests,
        args.stale_open_cutoff_ms,
        options,
        now_ms(),
    )?;
    let Some(execution) = args.execution else {
        return serde_json::to_value(plan).map_err(|error| error.to_string());
    };
    if plan.plan_fingerprint != execution.plan_fingerprint {
        return Err("git-clone-reclaim-plan-fingerprint-mismatch".into());
    }
    let approval = approve_git_clone_reclaim(
        &plan,
        &execution.confirmation,
        now_ms(),
        &execution.approved_by,
        &execution.rationale,
    )?;
    let result = execute_git_clone_reclaim(
        &plan,
        &approval,
        &args.retention_references,
        args.include_closed_pull_requests,
        args.stale_open_cutoff_ms,
        options,
        &execution.journal_path,
        now_ms(),
    )?;
    serde_json::to_value(result).map_err(|error| error.to_string())
}

fn main() {
    let raw = std::env::args_os().skip(1).collect::<Vec<_>>();
    match parse_args(&raw).and_then(|parsed| parsed.map(run).transpose()) {
        Ok(None) => println!("{USAGE}"),
        Ok(Some(output)) => println!("{}", serde_json::to_string_pretty(&output).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_authority_is_complete_or_rejected() {
        let plan_only = vec![
            "--repository-root".into(),
            "/tmp/clone".into(),
            "--reference-ref".into(),
            "refs/heads/main".into(),
        ];
        assert!(parse_args(&plan_only).unwrap().unwrap().execution.is_none());
        let mut incomplete = plan_only;
        incomplete.push("--execute".into());
        assert_eq!(
            parse_args(&incomplete).unwrap_err(),
            "git-clone-reclaim-execution-input-missing"
        );
    }
}
