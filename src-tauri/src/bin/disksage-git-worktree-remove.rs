//! Execute the existing fail-closed stale-worktree removal path from a terminal.
//!
//! The command re-audits immediately before mutation, requires the exact audit phrase, records
//! immutable approval/result evidence, and never deletes branches or runs `git worktree prune`.

use disksage_lib::{cloud, git_worktree, git_worktree_github_evidence};
use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "usage: disksage-git-worktree-remove \
--repository-root ABSOLUTE_PATH --reference-ref REF [--reference-ref REF ...] \
[--include-closed-pull-requests] [--stale-open-pull-request-cutoff-ms N] \
[--command-timeout-ms N] [--size-scan-timeout-ms N] \
[--max-worktrees N] [--max-entries-per-worktree N] [--max-active-pids N] \
[--enable-orca-protections --recent-write-window-secs N] \
[--orca-terminal-json ABSOLUTE_JSON] [--open-pr-head-oid OID] \
[--open-pr-head-branch NAME] [--lead-queue-file ABSOLUTE_PATH] \
[--assess-filesystem-protections] [--assess-unpushed-commits] [--assess-stash] \
--approved-removal-plan-fingerprint HEX64 \
--confirmation-exact-approval-phrase PHRASE --reviewed-by human:ID --rationale TEXT \
--record-root ABSOLUTE_PATH";

#[derive(Debug, PartialEq, Eq)]
struct Args {
    repository_root: PathBuf,
    retention_references: Vec<String>,
    include_closed_pull_requests: bool,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    command_timeout_ms: u64,
    size_scan_timeout_ms: u64,
    max_worktrees: usize,
    max_entries_per_worktree: u64,
    max_active_pids: usize,
    protection: disksage_lib::reclaim_protection::ProtectionContext,
    assess_filesystem_protections: bool,
    assess_unpushed_commits: bool,
    assess_stash: bool,
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
    let mut command_timeout_ms = None;
    let mut size_scan_timeout_ms = None;
    let mut max_worktrees = None;
    let mut max_entries_per_worktree = None;
    let mut max_active_pids = None;
    let mut protection = disksage_lib::reclaim_protection::ProtectionContext::default();
    let mut enable_orca_protections = false;
    let mut seen_recent_write_window = false;
    let mut seen_orca_terminal_json = false;
    let mut assess_filesystem_protections = false;
    let mut assess_unpushed_commits = false;
    let mut assess_stash = false;
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
            Some("--command-timeout-ms") if command_timeout_ms.is_none() => {
                command_timeout_ms = Some(
                    next_utf8(&mut args, "--command-timeout-ms")?
                        .parse()
                        .map_err(|_| "--command-timeout-ms must be an integer")?,
                )
            }
            Some("--command-timeout-ms") => return Err("duplicate option".into()),
            Some("--size-scan-timeout-ms") if size_scan_timeout_ms.is_none() => {
                size_scan_timeout_ms = Some(
                    next_utf8(&mut args, "--size-scan-timeout-ms")?
                        .parse()
                        .map_err(|_| "--size-scan-timeout-ms must be an integer")?,
                )
            }
            Some("--size-scan-timeout-ms") => return Err("duplicate option".into()),
            Some("--max-worktrees") if max_worktrees.is_none() => {
                max_worktrees = Some(
                    next_utf8(&mut args, "--max-worktrees")?
                        .parse()
                        .map_err(|_| "--max-worktrees must be an integer")?,
                )
            }
            Some("--max-worktrees") => return Err("duplicate option".into()),
            Some("--max-entries-per-worktree") if max_entries_per_worktree.is_none() => {
                max_entries_per_worktree = Some(
                    next_utf8(&mut args, "--max-entries-per-worktree")?
                        .parse()
                        .map_err(|_| "--max-entries-per-worktree must be an integer")?,
                )
            }
            Some("--max-entries-per-worktree") => return Err("duplicate option".into()),
            Some("--max-active-pids") if max_active_pids.is_none() => {
                max_active_pids = Some(
                    next_utf8(&mut args, "--max-active-pids")?
                        .parse()
                        .map_err(|_| "--max-active-pids must be an integer")?,
                )
            }
            Some("--max-active-pids") => return Err("duplicate option".into()),
            Some("--enable-orca-protections") if !enable_orca_protections => {
                enable_orca_protections = true
            }
            Some("--enable-orca-protections") => return Err("duplicate option".into()),
            Some("--recent-write-window-secs") if !seen_recent_write_window => {
                seen_recent_write_window = true;
                protection.recent_write_window_secs = Some(
                    next_utf8(&mut args, "--recent-write-window-secs")?
                        .parse()
                        .map_err(|_| "--recent-write-window-secs must be an integer")?,
                );
            }
            Some("--recent-write-window-secs") => return Err("duplicate option".into()),
            Some("--orca-terminal-json") if !seen_orca_terminal_json => {
                seen_orca_terminal_json = true;
                let path = next_path(&mut args, "--orca-terminal-json")?;
                if !path.is_absolute() {
                    return Err("--orca-terminal-json must be absolute".into());
                }
                let bytes = std::fs::read(&path)
                    .map_err(|_| "orca-terminal-json-read-failed".to_string())?;
                protection.orca_live_worktree_paths =
                    disksage_lib::reclaim_protection::parse_orca_terminal_worktree_paths(&bytes)?;
            }
            Some("--orca-terminal-json") => return Err("duplicate option".into()),
            Some("--open-pr-head-oid") => protection
                .open_pr_head_oids
                .push(next_utf8(&mut args, "--open-pr-head-oid")?),
            Some("--open-pr-head-branch") => protection
                .open_pr_head_branches
                .push(next_utf8(&mut args, "--open-pr-head-branch")?),
            Some("--lead-queue-file") => {
                let path = next_path(&mut args, "--lead-queue-file")?;
                if !path.is_absolute() {
                    return Err("--lead-queue-file must be absolute".into());
                }
                let text = std::fs::read_to_string(&path)
                    .map_err(|_| "lead-queue-file-read-failed".to_string())?;
                let workspace_root = path.parent().unwrap_or(path.as_path());
                protection.lead_queue_worktree_paths.extend(
                    disksage_lib::reclaim_protection::lead_queue_mentioned_paths(
                        &text,
                        workspace_root,
                    ),
                );
            }
            Some("--assess-filesystem-protections") if !assess_filesystem_protections => {
                assess_filesystem_protections = true
            }
            Some("--assess-filesystem-protections") => return Err("duplicate option".into()),
            Some("--assess-unpushed-commits") if !assess_unpushed_commits => {
                assess_unpushed_commits = true
            }
            Some("--assess-unpushed-commits") => return Err("duplicate option".into()),
            Some("--assess-stash") if !assess_stash => assess_stash = true,
            Some("--assess-stash") => return Err("duplicate option".into()),
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
    if enable_orca_protections {
        if protection.recent_write_window_secs.is_none() {
            return Err(
                "--enable-orca-protections requires --recent-write-window-secs (no silent default)"
                    .into(),
            );
        }
        assess_filesystem_protections = true;
        assess_unpushed_commits = true;
        assess_stash = true;
    }
    let defaults = git_worktree::GitWorktreeAuditOptions::default();

    Ok(ParseResult::Run(Args {
        repository_root,
        retention_references,
        include_closed_pull_requests,
        stale_open_pull_request_cutoff_ms,
        command_timeout_ms: command_timeout_ms.unwrap_or(defaults.command_timeout_ms),
        size_scan_timeout_ms: size_scan_timeout_ms.unwrap_or(defaults.size_scan_timeout_ms),
        max_worktrees: max_worktrees.unwrap_or(defaults.max_worktrees),
        max_entries_per_worktree: max_entries_per_worktree
            .unwrap_or(defaults.max_entries_per_worktree),
        max_active_pids: max_active_pids.unwrap_or(defaults.max_active_pids),
        protection,
        assess_filesystem_protections,
        assess_unpushed_commits,
        assess_stash,
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
    let options = git_worktree::GitWorktreeAuditOptions {
        command_timeout_ms: args.command_timeout_ms,
        size_scan_timeout_ms: args.size_scan_timeout_ms,
        max_worktrees: args.max_worktrees,
        max_entries_per_worktree: args.max_entries_per_worktree,
        max_active_pids: args.max_active_pids,
        protection: args.protection,
        assess_filesystem_protections: args.assess_filesystem_protections,
        assess_unpushed_commits: args.assess_unpushed_commits,
        assess_stash: args.assess_stash,
    };
    let audited_at_ms = cloud::system_now_ms();
    let evidence = git_worktree_github_evidence::collect(
        &args.repository_root,
        args.include_closed_pull_requests,
        args.stale_open_pull_request_cutoff_ms,
        options.clone(),
    )?;
    let report = git_worktree::audit_git_worktrees_with_pull_request_membership(
        &args.repository_root,
        &args.retention_references,
        &evidence.closed_heads,
        &evidence.stale_open_heads,
        &evidence.pull_request_commits,
        args.stale_open_pull_request_cutoff_ms,
        options.clone(),
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

    #[test]
    fn parser_preserves_custom_audit_resource_limits() {
        let mut args = valid_args();
        args.splice(
            4..4,
            [
                OsString::from("--max-worktrees"),
                OsString::from("17"),
                OsString::from("--max-entries-per-worktree"),
                OsString::from("2345"),
                OsString::from("--max-active-pids"),
                OsString::from("9"),
            ],
        );
        let ParseResult::Run(parsed) = parse_args(args).unwrap() else {
            panic!("runtime arguments must parse as a removal request");
        };
        assert_eq!(parsed.max_worktrees, 17);
        assert_eq!(parsed.max_entries_per_worktree, 2345);
        assert_eq!(parsed.max_active_pids, 9);
    }

    #[test]
    fn parser_requires_explicit_recent_write_window_for_orca_pack() {
        let mut no_window = valid_args();
        no_window.splice(4..4, [OsString::from("--enable-orca-protections")]);
        assert_eq!(
            parse_args(no_window).unwrap_err(),
            "--enable-orca-protections requires --recent-write-window-secs (no silent default)"
        );

        let mut with_window = valid_args();
        with_window.splice(
            4..4,
            [
                OsString::from("--enable-orca-protections"),
                OsString::from("--recent-write-window-secs"),
                OsString::from("3600"),
            ],
        );
        let ParseResult::Run(parsed) = parse_args(with_window).unwrap() else {
            panic!("Orca-protected removal request must parse")
        };
        assert_eq!(parsed.protection.recent_write_window_secs, Some(3600));
        assert!(parsed.assess_filesystem_protections);
        assert!(parsed.assess_unpushed_commits);
        assert!(parsed.assess_stash);
    }
}
