use disksage_lib::photo_similarity_audit::{
    collect_photo_similarity_audit, execute_photo_quarantine, plan_photo_quarantine,
    PhotoQuarantineSelection, PhotoSimilarityAuditReport, DEFAULT_MAX_ENTRIES,
};
use std::ffi::OsString;
use std::path::PathBuf;

const MAX_PHOTO_PRIVATE_REPORT_BYTES: usize = 64 * 1024 * 1024;
const EXECUTION_UNSUPPORTED: &str = "photo-audit-execution-unsupported-on-platform";
const AUDIT_OPTIONS_REQUIRE_AUDIT: &str = "photo-audit-audit-options-require-audit";

#[cfg(not(windows))]
const USAGE: &str = "Usage: disksage-photo-similarity-audit --root ABSOLUTE_PATH [--max-entries N] [--private-output PATH]\n\
       disksage-photo-similarity-audit --execute --root ABSOLUTE_PATH --private-report PATH \\\n          --select GROUP_FINGERPRINT=RELATIVE_PATH [...] --approval EXACT_PHRASE \\
         --rationale TEXT --journal-path PATH\n\
Groups non-identical photo candidates using exact DCT perceptual-hash and aspect-ratio evidence.\n\
Managed Photos libraries are never entered. Execution requires one selected survivor per group and\n\
moves only the remaining files to OS Trash; it never permanently deletes them.";

#[cfg(windows)]
const USAGE: &str = "Usage: disksage-photo-similarity-audit --root ABSOLUTE_PATH [--max-entries N] [--private-output PATH]\n\
Groups non-identical photo candidates using exact DCT perceptual-hash and aspect-ratio evidence.\n\
Managed Photos libraries are never entered. This Windows build is audit-only because active-use\n\
proof for quarantine execution is not yet available.";

#[derive(Debug)]
struct Args {
    root: PathBuf,
    max_entries: usize,
    private_output: Option<PathBuf>,
    private_report: Option<PathBuf>,
    selections: Vec<PhotoQuarantineSelection>,
    approval: Option<String>,
    rationale: Option<String>,
    journal_path: Option<PathBuf>,
    execute: bool,
}

fn next_value(
    arguments: &mut impl Iterator<Item = OsString>,
    error: &'static str,
) -> Result<OsString, String> {
    arguments.next().ok_or_else(|| error.into())
}

fn parse_selection(value: OsString) -> Result<PhotoQuarantineSelection, String> {
    let value = value
        .into_string()
        .map_err(|_| "photo-audit-selection-invalid".to_string())?;
    let (group_fingerprint, survivor_relative_path) = value
        .split_once('=')
        .ok_or_else(|| "photo-audit-selection-invalid".to_string())?;
    if group_fingerprint.len() != 64 || survivor_relative_path.is_empty() {
        return Err("photo-audit-selection-invalid".into());
    }
    Ok(PhotoQuarantineSelection {
        group_fingerprint: group_fingerprint.into(),
        survivor_relative_path: survivor_relative_path.into(),
    })
}

fn parse_args_with_execution_support(
    arguments: impl IntoIterator<Item = OsString>,
    execution_supported: bool,
) -> Result<Args, String> {
    let mut arguments = arguments.into_iter();
    let mut root = None;
    let mut max_entries = DEFAULT_MAX_ENTRIES;
    let mut max_entries_set = false;
    let mut private_output = None;
    let mut private_report = None;
    let mut selections = Vec::new();
    let mut approval = None;
    let mut rationale = None;
    let mut journal_path = None;
    let mut execute = false;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--root") if root.is_none() => {
                root = Some(PathBuf::from(next_value(
                    &mut arguments,
                    "photo-audit-root-missing",
                )?));
            }
            Some("--max-entries") => {
                max_entries = next_value(&mut arguments, "photo-audit-max-entries-missing")?
                    .into_string()
                    .map_err(|_| "photo-audit-max-entries-invalid".to_string())?
                    .parse()
                    .map_err(|_| "photo-audit-max-entries-invalid".to_string())?;
                max_entries_set = true;
            }
            Some("--private-output") if private_output.is_none() => {
                private_output = Some(PathBuf::from(next_value(
                    &mut arguments,
                    "photo-audit-private-output-missing",
                )?));
            }
            Some("--private-report") if private_report.is_none() => {
                private_report = Some(PathBuf::from(next_value(
                    &mut arguments,
                    "photo-audit-private-report-missing",
                )?));
            }
            Some("--select") => selections.push(parse_selection(next_value(
                &mut arguments,
                "photo-audit-selection-missing",
            )?)?),
            Some("--approval") if approval.is_none() => {
                approval = Some(
                    next_value(&mut arguments, "photo-audit-approval-missing")?
                        .into_string()
                        .map_err(|_| "photo-audit-approval-invalid".to_string())?,
                );
            }
            Some("--rationale") if rationale.is_none() => {
                rationale = Some(
                    next_value(&mut arguments, "photo-audit-rationale-missing")?
                        .into_string()
                        .map_err(|_| "photo-audit-rationale-invalid".to_string())?,
                );
            }
            Some("--journal-path") if journal_path.is_none() => {
                journal_path = Some(PathBuf::from(next_value(
                    &mut arguments,
                    "photo-audit-journal-path-missing",
                )?));
            }
            Some("--execute") if !execute => {
                if !execution_supported {
                    return Err(EXECUTION_UNSUPPORTED.into());
                }
                execute = true;
            }
            Some("--help") | Some("-h") => return Err(USAGE.into()),
            _ => return Err("photo-audit-argument-invalid".into()),
        }
    }
    if execute && (private_output.is_some() || max_entries_set) {
        return Err(AUDIT_OPTIONS_REQUIRE_AUDIT.into());
    }
    Ok(Args {
        root: root.ok_or_else(|| "photo-audit-root-missing".to_string())?,
        max_entries,
        private_output,
        private_report,
        selections,
        approval,
        rationale,
        journal_path,
        execute,
    })
}

fn parse_args(arguments: impl IntoIterator<Item = OsString>) -> Result<Args, String> {
    parse_args_with_execution_support(arguments, cfg!(not(windows)))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn read_private_report(path: &PathBuf) -> Result<PhotoSimilarityAuditReport, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "photo-audit-private-report-unavailable".to_string())?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_PHOTO_PRIVATE_REPORT_BYTES as u64
    {
        return Err("photo-audit-private-report-unsafe".into());
    }
    let bytes =
        std::fs::read(path).map_err(|_| "photo-audit-private-report-read-failed".to_string())?;
    let report: PhotoSimilarityAuditReport = serde_json::from_slice(&bytes)
        .map_err(|_| "photo-audit-private-report-invalid".to_string())?;
    if report.groups.len() > disksage_lib::photo_similarity_audit::MAX_ENTRIES
        || report
            .groups
            .iter()
            .any(|group| group.members.len() > disksage_lib::photo_similarity_audit::MAX_ENTRIES)
    {
        return Err("photo-audit-private-report-structure-too-large".into());
    }
    Ok(report)
}

fn write_private_report(
    source_root: &PathBuf,
    path: &PathBuf,
    report: &PhotoSimilarityAuditReport,
) -> Result<(), String> {
    if std::fs::symlink_metadata(path).is_ok() {
        return Err("photo-audit-private-output-exists".into());
    }
    disksage_lib::private_evidence::write_private_json_create_new_with_limit(
        source_root,
        path,
        report,
        MAX_PHOTO_PRIVATE_REPORT_BYTES,
    )
    .map(|_| ())
    .map_err(|error| match error.as_str() {
        "private-evidence-parent-unavailable" => {
            "photo-audit-private-output-parent-unavailable".to_string()
        }
        "private-evidence-parent-unsafe" | "private-evidence-parent-writable-by-others" => {
            "photo-audit-private-output-parent-unsafe".to_string()
        }
        "private-evidence-inside-source-root" | "private-evidence-name-invalid" => {
            "photo-audit-private-output-unsafe".to_string()
        }
        "private-evidence-too-large" => "photo-audit-private-output-too-large".to_string(),
        "private-evidence-secure-mode-unsupported" => {
            "photo-audit-private-output-secure-mode-unsupported".to_string()
        }
        _ => "photo-audit-private-output-write-failed".to_string(),
    })
}

fn run(arguments: impl IntoIterator<Item = OsString>) -> Result<serde_json::Value, String> {
    let args = parse_args(arguments)?;
    if args.execute {
        let private_report = args
            .private_report
            .as_ref()
            .ok_or_else(|| "photo-audit-private-report-missing".to_string())?;
        let report = read_private_report(private_report)?;
        let plan = plan_photo_quarantine(&report, &args.selections)?;
        let receipt = execute_photo_quarantine(
            &args.root,
            &report,
            &plan,
            args.approval
                .as_deref()
                .ok_or_else(|| "photo-audit-approval-missing".to_string())?,
            args.rationale
                .as_deref()
                .ok_or_else(|| "photo-audit-rationale-missing".to_string())?,
            args.journal_path
                .as_deref()
                .ok_or_else(|| "photo-audit-journal-path-missing".to_string())?,
            now_ms(),
        )?;
        return serde_json::to_value(receipt)
            .map_err(|_| "photo-audit-receipt-serialize-failed".into());
    }
    if args.private_report.is_some()
        || !args.selections.is_empty()
        || args.approval.is_some()
        || args.rationale.is_some()
        || args.journal_path.is_some()
    {
        return Err("photo-audit-execution-arguments-require-execute".into());
    }
    let report = collect_photo_similarity_audit(&args.root, now_ms(), args.max_entries)?;
    if let Some(path) = args.private_output {
        write_private_report(&args.root, &path, &report)?;
    }
    Ok(serde_json::json!({
        "schema_version": report.schema_version,
        "output_mode": "photo-similarity-audit-summary",
        "observed_at_ms": report.observed_at_ms,
        "evidence_complete": report.evidence_complete,
        "entries_seen": report.entries_seen,
        "decoded_photo_count": report.decoded_photo_count,
        "group_count": report.group_count,
        "managed_library_excluded_count": report.managed_library_excluded_count,
        "dataless_photo_excluded_count": report.dataless_photo_excluded_count,
        "issue_counts": report.issue_counts,
        "audit_fingerprint": report.audit_fingerprint,
        "perceptual_algorithm": report.perceptual_algorithm,
        "grouping_policy": report.grouping_policy,
        "survivor_policy": report.survivor_policy,
        "automatic_delete_allowed": false,
        "mutation_performed": false,
        "next_action": "비공개 보고서에서 각 후보의 실제 미리보기와 측정값을 비교한 뒤 그룹마다 보존할 원본을 하나 선택하세요."
    }))
}

fn main() {
    match run(std::env::args_os().skip(1)) {
        Ok(output) => println!("{}", serde_json::to_string_pretty(&output).unwrap()),
        Err(error) if error == USAGE => {
            println!("{USAGE}");
        }
        Err(error) => {
            eprintln!("DiskSage photo similarity audit: {error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_requires_explicit_execution_arguments() {
        let args = parse_args([
            OsString::from("--root"),
            OsString::from("/tmp/photos"),
            OsString::from("--max-entries"),
            OsString::from("100"),
        ])
        .unwrap();
        assert_eq!(args.max_entries, 100);
        assert!(!args.execute);
        assert!(parse_selection(OsString::from("bad")).is_err());
    }

    #[test]
    fn unsupported_execution_is_rejected_before_domain_work() {
        let error = parse_args_with_execution_support(
            [
                OsString::from("--execute"),
                OsString::from("--root"),
                OsString::from("/photos"),
            ],
            false,
        )
        .unwrap_err();
        assert_eq!(error, EXECUTION_UNSUPPORTED);
    }

    #[cfg(not(windows))]
    #[test]
    fn help_documents_selection_without_patch_residue() {
        assert!(USAGE.contains("--select GROUP_FINGERPRINT=RELATIVE_PATH"));
        assert!(!USAGE.lines().any(|line| line.starts_with('+')));
    }

    #[cfg(windows)]
    #[test]
    fn windows_execution_is_not_advertised_or_accepted() {
        assert!(!USAGE.contains("--execute"));
        let error = parse_args([
            OsString::from("--execute"),
            OsString::from("--root"),
            OsString::from(r"C:\photos"),
        ])
        .unwrap_err();
        assert_eq!(error, EXECUTION_UNSUPPORTED);
    }
}
