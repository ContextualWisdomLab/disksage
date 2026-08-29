//! Execute the existing fail-closed stale-worktree removal path from a terminal.
//!
//! The command re-audits immediately before mutation, requires the exact audit phrase, records
//! immutable approval/result evidence, and never deletes branches or runs `git worktree prune`.

use disksage_lib::{cloud, git_worktree};
use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "usage: disksage-git-worktree-remove \
--repository-root ABSOLUTE_PATH --reference-ref REF [--reference-ref REF ...] \
[--include-closed-pull-requests] [--stale-open-pull-request-cutoff-ms N] \
--approved-removal-plan-fingerprint HEX64 \
--confirmation-exact-approval-phrase PHRASE --reviewed-by human:ID --rationale TEXT \
--record-root ABSOLUTE_PATH";

#[derive(Debug, PartialEq, Eq)]
struct Args {
    repository_root: PathBuf,
    retention_references: Vec<String>,
    include_closed_pull_requests: bool,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    plan_fingerprint: String,
    confirmation_phrase: String,
    reviewed_by: String,
    rationale: String,
    record_root: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
enum ParseResult {
    Run(Args),
    Help,
}

fn next_utf8(args: &mut impl Iterator<Item = OsString>, option: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{option} requires a value"))?
        .into_string()
        .map_err(|_| format!("{option} requires a UTF-8 value"))
}

fn next_path(args: &mut impl Iterator<Item = OsString>, option: &str) -> Result<PathBuf, String> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{option} requires an absolute path"))
}

fn parse_args(raw_args: impl IntoIterator<Item = OsString>) -> Result<ParseResult, String> {
    let mut repository_root = None;
    let mut retention_references = Vec::new();
    let mut plan_fingerprint = None;
    let mut include_closed_pull_requests = false;
    let mut stale_open_pull_request_cutoff_ms = None;
    let mut confirmation_phrase = None;
    let mut reviewed_by = None;
    let mut rationale = None;
    let mut record_root = None;
    let mut args = raw_args.into_iter();

    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--repository-root") => {
                repository_root = Some(next_path(&mut args, "--repository-root")?)
            }
            Some("--reference-ref") => {
                retention_references.push(next_utf8(&mut args, "--reference-ref")?)
            }
            Some("--include-closed-pull-requests") if !include_closed_pull_requests => {
                include_closed_pull_requests = true
            }
            Some("--include-closed-pull-requests") => return Err("duplicate option".into()),
            Some("--stale-open-pull-request-cutoff-ms")
                if stale_open_pull_request_cutoff_ms.is_none() =>
            {
                stale_open_pull_request_cutoff_ms = Some(
                    next_utf8(&mut args, "--stale-open-pull-request-cutoff-ms")?
                        .parse()
                        .map_err(|_| "--stale-open-pull-request-cutoff-ms must be an integer")?,
                )
            }
            Some("--stale-open-pull-request-cutoff-ms") => return Err("duplicate option".into()),
            Some("--approved-removal-plan-fingerprint") => {
                plan_fingerprint =
                    Some(next_utf8(&mut args, "--approved-removal-plan-fingerprint")?)
            }
            Some("--confirmation-exact-approval-phrase") => {
                confirmation_phrase = Some(next_utf8(
                    &mut args,
                    "--confirmation-exact-approval-phrase",
                )?)
            }
            Some("--reviewed-by") => reviewed_by = Some(next_utf8(&mut args, "--reviewed-by")?),
            Some("--rationale") => rationale = Some(next_utf8(&mut args, "--rationale")?),
            Some("--record-root") => record_root = Some(next_path(&mut args, "--record-root")?),
            Some("-h" | "--help") => return Ok(ParseResult::Help),
            Some(option) => return Err(format!("unknown option: {option}\n{USAGE}")),
            None => return Err("option must be valid UTF-8".into()),
        }
    }

    let repository_root =
        repository_root.ok_or_else(|| format!("--repository-root is required\n{USAGE}"))?;
    if !repository_root.is_absolute() {
        return Err("--repository-root must be absolute".into());
    }
    if retention_references.is_empty() {
        return Err(format!("at least one --reference-ref is required\n{USAGE}"));
    }
    let plan_fingerprint = plan_fingerprint
        .ok_or_else(|| format!("--approved-removal-plan-fingerprint is required\n{USAGE}"))?;
    if plan_fingerprint.len() != 64
        || !plan_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("--approved-removal-plan-fingerprint must be 64 hexadecimal characters".into());
    }
    let confirmation_phrase = confirmation_phrase
        .ok_or_else(|| format!("--confirmation-exact-approval-phrase is required\n{USAGE}"))?;
    let reviewed_by = reviewed_by.ok_or_else(|| format!("--reviewed-by is required\n{USAGE}"))?;
    let rationale = rationale.ok_or_else(|| format!("--rationale is required\n{USAGE}"))?;
    let record_root = record_root.ok_or_else(|| format!("--record-root is required\n{USAGE}"))?;
    if !record_root.is_absolute() {
        return Err("--record-root must be absolute".into());
    }

    Ok(ParseResult::Run(Args {
        repository_root,
        retention_references,
        include_closed_pull_requests,
        stale_open_pull_request_cutoff_ms,
        plan_fingerprint,
        confirmation_phrase,
        reviewed_by,
        rationale,
        record_root,
    }))
}

#[derive(serde::Serialize)]
struct RemovalOutput {
    action: &'static str,
    report: git_worktree::GitWorktreeAuditReport,
    approval: git_worktree::GitWorktreeRemovalApproval,
    approval_path: String,
    result: git_worktree::GitWorktreeRemovalResult,
    result_path: Option<String>,
    result_record_error: Option<String>,
}

fn execute(args: Args) -> Result<RemovalOutput, String> {
    let options = git_worktree::GitWorktreeAuditOptions::default();
    let audited_at_ms = cloud::system_now_ms();
    let closed_heads = if args.include_closed_pull_requests {
        git_worktree::github_closed_pull_request_heads_with_options(&args.repository_root, options)?
    } else {
        Default::default()
    };
    let mut pull_request_commits =
        if args.include_closed_pull_requests || args.stale_open_pull_request_cutoff_ms.is_some() {
            git_worktree::github_pull_request_commit_membership(&args.repository_root, options)?
        } else {
            Default::default()
        };
    if !args.include_closed_pull_requests {
        pull_request_commits.completed.clear();
    }
    let stale_open_heads = if let Some(cutoff_ms) = args.stale_open_pull_request_cutoff_ms {
        git_worktree::github_stale_open_pull_request_heads(
            &args.repository_root,
            cutoff_ms,
            options.command_timeout_ms,
        )?
    } else {
        Default::default()
    };
    let report = git_worktree::audit_git_worktrees_with_pull_request_membership(
        &args.repository_root,
        &args.retention_references,
        &closed_heads,
        &stale_open_heads,
        &pull_request_commits,
        args.stale_open_pull_request_cutoff_ms,
        options,
        audited_at_ms,
    )?;
    if report.removal_plan_fingerprint != args.plan_fingerprint {
        return Err("git-worktree-removal-plan-fingerprint-mismatch".into());
    }
    let approval = git_worktree::approve_stale_worktree_removal(
        &report,
        &args.confirmation_phrase,
        cloud::system_now_ms(),
        &args.reviewed_by,
        &args.rationale,
    )?;
    let record_dir = git_worktree::prepare_worktree_record_directory(
        &args.record_root,
        &report,
        "git-worktree-removals",
    )?;
    let approval_path = git_worktree::write_immutable_worktree_record(
        &record_dir,
        &format!("{}.approval.json", approval.approval_id),
        &approval,
    )?;
    let result = git_worktree::execute_stale_worktree_removal_with_github_pull_requests(
        &report,
        &approval,
        &args.confirmation_phrase,
        args.include_closed_pull_requests,
        args.stale_open_pull_request_cutoff_ms,
        options,
        cloud::system_now_ms(),
    )?;
    let result_record = git_worktree::write_immutable_worktree_record(
        &record_dir,
        &format!("{}.result.json", result.result_id),
        &result,
    );
    let (result_path, result_record_error) = match result_record {
        Ok(path) => (Some(path.to_string_lossy().into_owned()), None),
        Err(error) => (None, Some(error)),
    };
    Ok(RemovalOutput {
        action: "remove-stale-git-worktrees",
        report,
        approval,
        approval_path: approval_path.to_string_lossy().into_owned(),
        result,
        result_path,
        result_record_error,
    })
}

fn main() {
    let args = match parse_args(std::env::args_os().skip(1)) {
        Ok(ParseResult::Run(args)) => args,
        Ok(ParseResult::Help) => {
            println!("{USAGE}");
            return;
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(64);
        }
    };
    match execute(args) {
        Ok(output) => match serde_json::to_string_pretty(&output) {
            Ok(encoded) => println!("{encoded}"),
            Err(_) => std::process::exit(70),
        },
        Err(error) => {
            eprintln!("disksage-git-worktree-remove: {error}");
            std::process::exit(65);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_args() -> Vec<OsString> {
        vec![
            "--repository-root".into(),
            "/tmp/repository".into(),
            "--reference-ref".into(),
            "origin/develop".into(),
            "--approved-removal-plan-fingerprint".into(),
            "a".repeat(64).into(),
            "--confirmation-exact-approval-phrase".into(),
            "DiskSage stale worktree approval".into(),
            "--reviewed-by".into(),
            "human:test".into(),
            "--rationale".into(),
            "merged and inactive".into(),
            "--record-root".into(),
            "/tmp/records".into(),
        ]
    }

    #[test]
    fn parser_requires_explicit_mutation_boundary() {
        assert!(parse_args(Vec::<OsString>::new()).is_err());
        assert!(matches!(parse_args(valid_args()), Ok(ParseResult::Run(_))));
    }

    #[test]
    fn help_is_a_successful_terminal_parse_result() {
        assert_eq!(
            parse_args([OsString::from("--help")]).unwrap(),
            ParseResult::Help
        );
    }

    #[test]
    fn parser_rejects_non_absolute_roots_and_bad_fingerprint() {
        let mut args = valid_args();
        args[1] = "relative".into();
        assert!(parse_args(args).is_err());

        let mut args = valid_args();
        args[5] = "bad".into();
        assert!(parse_args(args).is_err());
    }
}
