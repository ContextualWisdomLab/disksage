//! Remove only Git worktree registrations whose paths Git proves no longer exist.

use disksage_lib::private_evidence::write_private_json_create_new;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const USAGE: &str = "usage: disksage-git-worktree-metadata-prune --repository-root ABSOLUTE_PATH [--execute --confirm EXACT_PHRASE --rationale TEXT --record-path ABSOLUTE_PATH]";
const PREFIX: &str = "Removing worktrees/";
const SUFFIX: &str = ": gitdir file points to non-existent location";

#[derive(Debug, PartialEq, Eq)]
struct Args {
    repository_root: PathBuf,
    execute: bool,
    confirm: Option<String>,
    rationale: Option<String>,
    record_path: Option<PathBuf>,
}

#[derive(Debug, serde::Serialize)]
struct Plan {
    schema_kind: &'static str,
    schema_version: u32,
    repository_fingerprint: String,
    candidate_count: usize,
    plan_fingerprint: String,
    exact_approval_phrase: Option<String>,
    evidence_complete: bool,
    filesystem_path_delete_executed: bool,
    branch_delete_executed: bool,
    git_object_delete_executed: bool,
}

#[derive(Debug, serde::Serialize)]
struct Receipt {
    schema_kind: &'static str,
    schema_version: u32,
    plan_fingerprint: String,
    candidate_count: usize,
    remaining_candidate_count: usize,
    metadata_prune_executed: bool,
    verification_complete: bool,
    rationale: String,
    filesystem_path_delete_executed: bool,
    branch_delete_executed: bool,
    git_object_delete_executed: bool,
}

fn value(args: &[OsString], index: &mut usize, option: &str) -> Result<OsString, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{option} requires a value"))
}

fn parse_args(args: &[OsString]) -> Result<Args, String> {
    let mut repository_root = None;
    let mut execute = false;
    let mut confirm = None;
    let mut rationale = None;
    let mut record_path = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str() {
            Some("--repository-root") if repository_root.is_none() => {
                repository_root =
                    Some(PathBuf::from(value(args, &mut index, "--repository-root")?));
            }
            Some("--execute") if !execute => execute = true,
            Some("--confirm") if confirm.is_none() => {
                confirm = Some(
                    value(args, &mut index, "--confirm")?
                        .into_string()
                        .map_err(|_| "--confirm requires UTF-8")?,
                );
            }
            Some("--rationale") if rationale.is_none() => {
                rationale = Some(
                    value(args, &mut index, "--rationale")?
                        .into_string()
                        .map_err(|_| "--rationale requires UTF-8")?,
                );
            }
            Some("--record-path") if record_path.is_none() => {
                record_path = Some(PathBuf::from(value(args, &mut index, "--record-path")?));
            }
            Some("--help" | "-h") => return Err(USAGE.into()),
            Some(_) => return Err(format!("invalid or duplicate option\n{USAGE}")),
            None => return Err("option must be valid UTF-8".into()),
        }
        index += 1;
    }
    let repository_root = repository_root.ok_or_else(|| USAGE.to_string())?;
    if !repository_root.is_absolute() {
        return Err("--repository-root must be absolute".into());
    }
    if execute {
        if confirm.is_none() || rationale.is_none() || record_path.is_none() {
            return Err("--execute requires --confirm, --rationale, and --record-path".into());
        }
        if !record_path.as_ref().is_some_and(|path| path.is_absolute()) {
            return Err("--record-path must be absolute".into());
        }
    } else if confirm.is_some() || rationale.is_some() || record_path.is_some() {
        return Err("mutation arguments require --execute".into());
    }
    Ok(Args {
        repository_root,
        execute,
        confirm,
        rationale,
        record_path,
    })
}

fn git(repository_root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository_root)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .output()
        .map_err(|_| "git-worktree-metadata-command-unavailable".to_string())?;
    if !output.status.success() || (!output.stdout.is_empty() && !output.stderr.is_empty()) {
        return Err("git-worktree-metadata-command-failed".into());
    }
    String::from_utf8(if output.stdout.is_empty() {
        output.stderr
    } else {
        output.stdout
    })
    .map_err(|_| "git-worktree-metadata-output-invalid".into())
}

fn fingerprint(fields: impl IntoIterator<Item = impl AsRef<[u8]>>) -> String {
    let mut hasher = Sha256::new();
    for field in fields {
        let field = field.as_ref();
        hasher.update((field.len() as u64).to_le_bytes());
        hasher.update(field);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn plan(repository_root: &Path) -> Result<(PathBuf, Plan), String> {
    let repository_root = std::fs::canonicalize(repository_root)
        .map_err(|_| "git-worktree-metadata-repository-unavailable".to_string())?;
    let common = git(
        &repository_root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let common = std::fs::canonicalize(common.trim())
        .map_err(|_| "git-worktree-metadata-common-dir-unavailable".to_string())?;
    let dry_run = git(
        &repository_root,
        &[
            "worktree",
            "prune",
            "--dry-run",
            "--verbose",
            "--expire",
            "now",
        ],
    )?;
    let mut candidates = dry_run
        .lines()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if candidates.iter().any(|line| {
        let Some(name) = line
            .strip_prefix(PREFIX)
            .and_then(|line| line.strip_suffix(SUFFIX))
        else {
            return true;
        };
        name.is_empty()
            || name.len() > 255
            || name.contains('/')
            || name.chars().any(char::is_control)
    }) {
        return Err("git-worktree-metadata-dry-run-evidence-invalid".into());
    }
    candidates.sort_unstable();
    candidates.dedup();
    let repository_fingerprint = fingerprint([common.as_os_str().as_encoded_bytes()]);
    let plan_fingerprint = fingerprint(
        std::iter::once(b"disksage.git-worktree-metadata-prune.v1".as_slice())
            .chain(std::iter::once(repository_fingerprint.as_bytes()))
            .chain(candidates.iter().map(|line| line.as_bytes())),
    );
    let exact_approval_phrase = (!candidates.is_empty()).then(|| {
        format!(
            "DiskSage stale worktree metadata {} 승인 {plan_fingerprint}",
            candidates.len()
        )
    });
    Ok((
        repository_root,
        Plan {
            schema_kind: "disksage.git-worktree-metadata-prune-plan",
            schema_version: 1,
            repository_fingerprint,
            candidate_count: candidates.len(),
            plan_fingerprint,
            exact_approval_phrase,
            evidence_complete: true,
            filesystem_path_delete_executed: false,
            branch_delete_executed: false,
            git_object_delete_executed: false,
        },
    ))
}

fn run(args: Args) -> Result<serde_json::Value, String> {
    let (repository_root, current_plan) = plan(&args.repository_root)?;
    if !args.execute {
        return serde_json::to_value(current_plan)
            .map_err(|_| "git-worktree-metadata-json-failed".into());
    }
    let phrase = current_plan
        .exact_approval_phrase
        .as_deref()
        .ok_or("git-worktree-metadata-candidate-set-empty")?;
    if args.confirm.as_deref() != Some(phrase) {
        return Err("git-worktree-metadata-confirmation-mismatch".into());
    }
    let rationale = args.rationale.unwrap();
    if rationale.trim() != rationale
        || rationale.is_empty()
        || rationale.len() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("git-worktree-metadata-rationale-invalid".into());
    }
    git(
        &repository_root,
        &["worktree", "prune", "--verbose", "--expire", "now"],
    )?;
    let (_, after) = plan(&repository_root)?;
    let receipt = Receipt {
        schema_kind: "disksage.git-worktree-metadata-prune-receipt",
        schema_version: 1,
        plan_fingerprint: current_plan.plan_fingerprint,
        candidate_count: current_plan.candidate_count,
        remaining_candidate_count: after.candidate_count,
        metadata_prune_executed: true,
        verification_complete: after.candidate_count == 0,
        rationale,
        filesystem_path_delete_executed: false,
        branch_delete_executed: false,
        git_object_delete_executed: false,
    };
    let record_path = args.record_path.unwrap();
    write_private_json_create_new(&repository_root, &record_path, &receipt)
        .map_err(|_| "git-worktree-metadata-record-write-failed".to_string())?;
    serde_json::to_value(receipt).map_err(|_| "git-worktree-metadata-json-failed".into())
}

fn main() {
    let result = parse_args(&std::env::args_os().skip(1).collect::<Vec<_>>()).and_then(run);
    match result {
        Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap()),
        Err(error) if error == USAGE => println!("{USAGE}"),
        Err(error) => {
            eprintln!("disksage-git-worktree-metadata-prune: {error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_keeps_planning_read_only_and_bounds_execution() {
        let root = std::env::temp_dir();
        let plan = parse_args(&["--repository-root".into(), root.clone().into()]).unwrap();
        assert!(!plan.execute);
        assert!(
            parse_args(&["--repository-root".into(), root.into(), "--execute".into()]).is_err()
        );
        assert!(parse_args(&["--repository-root".into(), "relative".into()]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn execution_prunes_only_missing_registration_and_records_verification() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repository");
        let linked = temp.path().join("linked");
        std::fs::create_dir(&repository).unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "DiskSage Test"],
            vec!["config", "user.email", "disksage@example.invalid"],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&repository)
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(repository.join("tracked"), b"safe\n").unwrap();
        assert!(Command::new("git")
            .args(["add", "tracked"])
            .current_dir(&repository)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-q", "-m", "fixture"])
            .current_dir(&repository)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args([
                "worktree",
                "add",
                "-q",
                linked.to_str().unwrap(),
                "-b",
                "stale"
            ])
            .current_dir(&repository)
            .status()
            .unwrap()
            .success());
        std::fs::remove_dir_all(&linked).unwrap();

        let (_, planned) = plan(&repository).unwrap();
        assert_eq!(planned.candidate_count, 1);
        let record_path = temp.path().join("receipt.json");
        let output = run(Args {
            repository_root: repository.clone(),
            execute: true,
            confirm: planned.exact_approval_phrase,
            rationale: Some("Missing worktree registration reviewed".into()),
            record_path: Some(record_path.clone()),
        })
        .unwrap();

        assert_eq!(output["remaining_candidate_count"], 0);
        assert_eq!(output["filesystem_path_delete_executed"], false);
        assert!(record_path.is_file());
        assert_eq!(plan(&repository).unwrap().1.candidate_count, 0);
    }
}
