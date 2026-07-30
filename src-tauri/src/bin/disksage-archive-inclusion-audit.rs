//! Read-only, path-redacted batch audit for content inclusion across local ZIP archives.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use disksage_lib::archive_git_tree::{
    audit_zip_content_inclusion_batch, export_naruon_archive_inclusion_lineage,
    ArchiveContentBatchOptions, ArchiveContentInclusionBatchReport, ArchiveTreeRootMode,
    NaruonArchiveInclusionLineageEnvelope,
};
use disksage_lib::private_evidence::{write_private_json_create_new, PrivateEvidenceReceipt};

const DEFAULT_MAX_ARCHIVES: usize = 128;
const DEFAULT_MAX_PAIRS: usize = 8_192;
const DEFAULT_MAX_DISCOVERY_ENTRIES: usize = 200_000;
const MAX_DISCOVERY_ENTRIES: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    root: PathBuf,
    private_output: Option<PathBuf>,
    max_archives: usize,
    max_pairs: usize,
    max_discovery_entries: usize,
    strip_shared_root: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct PublicSummary {
    schema_kind: &'static str,
    version: u32,
    generated_at_ms: u64,
    local_paths_redacted: bool,
    archive_names_redacted: bool,
    root_mode: String,
    archive_input_count: usize,
    archive_valid_count: usize,
    issue_count: usize,
    pair_comparison_count: usize,
    inclusion_relation_count: usize,
    strict_inclusion_relation_count: usize,
    identical_relation_count: usize,
    reclaim_review_candidate_count: usize,
    reclaim_review_compressed_bytes: u64,
    evidence_complete: bool,
    batch_fingerprint_sha256: String,
    naruon_lineage: NaruonArchiveInclusionLineageEnvelope,
    private_report: Option<PrivateEvidenceReceipt>,
    filesystem_mutation_executed: bool,
    deletion_authorized: bool,
    notices: Vec<String>,
}

fn usage() -> &'static str {
    "DiskSage archive inclusion audit: usage: disksage-archive-inclusion-audit --root ABSOLUTE_DIRECTORY [--private-output NEW_ABSOLUTE_JSON_PATH] [--max-archives N] [--max-pairs N] [--max-discovery-entries N] [--strip-shared-root]"
}

fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

fn positive_usize(value: String, flag: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{flag} 값이 올바르지 않음"))
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut root = None;
    let mut private_output = None;
    let mut max_archives = DEFAULT_MAX_ARCHIVES;
    let mut max_pairs = DEFAULT_MAX_PAIRS;
    let mut max_discovery_entries = DEFAULT_MAX_DISCOVERY_ENTRIES;
    let mut strip_shared_root = false;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => root = Some(PathBuf::from(value(args, &mut index, "--root")?)),
            "--private-output" => {
                private_output = Some(PathBuf::from(value(args, &mut index, "--private-output")?))
            }
            "--max-archives" => {
                max_archives =
                    positive_usize(value(args, &mut index, "--max-archives")?, "--max-archives")?
            }
            "--max-pairs" => {
                max_pairs = positive_usize(value(args, &mut index, "--max-pairs")?, "--max-pairs")?
            }
            "--max-discovery-entries" => {
                max_discovery_entries = positive_usize(
                    value(args, &mut index, "--max-discovery-entries")?,
                    "--max-discovery-entries",
                )?
            }
            "--strip-shared-root" => strip_shared_root = true,
            "--help" | "-h" => return Err(usage().into()),
            unknown => return Err(format!("알 수 없는 인자: {unknown}")),
        }
        index += 1;
    }
    if max_discovery_entries > MAX_DISCOVERY_ENTRIES {
        return Err("--max-discovery-entries 값이 상한을 초과함".into());
    }
    let root = root.ok_or_else(|| "--root 값이 필요함".to_string())?;
    if !root.is_absolute() {
        return Err("--root는 절대 경로여야 함".into());
    }
    if private_output
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err("--private-output은 절대 경로여야 함".into());
    }
    Ok(Args {
        root,
        private_output,
        max_archives,
        max_pairs,
        max_discovery_entries,
        strip_shared_root,
    })
}

fn canonical_root(path: &Path) -> Result<PathBuf, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "archive-batch-root-unavailable".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("archive-batch-root-unsafe".into());
    }
    std::fs::canonicalize(path).map_err(|_| "archive-batch-root-unavailable".to_string())
}

fn discover_zip_paths(
    root: &Path,
    max_entries: usize,
    max_archives: usize,
) -> Result<Vec<PathBuf>, String> {
    let mut stack = vec![root.to_path_buf()];
    let mut visited = 0usize;
    let mut archives = Vec::new();
    while let Some(directory) = stack.pop() {
        let mut children = std::fs::read_dir(&directory)
            .map_err(|_| "archive-batch-discovery-read-failed".to_string())?
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(|_| "archive-batch-discovery-read-failed".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        children.sort();
        for path in children {
            visited += 1;
            if visited > max_entries {
                return Err("archive-batch-discovery-entry-limit".into());
            }
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|_| "archive-batch-discovery-metadata-failed".to_string())?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                stack.push(path);
            } else if metadata.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
            {
                archives.push(path);
                if archives.len() > max_archives {
                    return Err("archive-batch-discovery-archive-limit".into());
                }
            }
        }
    }
    archives.sort();
    Ok(archives)
}

fn public_summary(
    report: &ArchiveContentInclusionBatchReport,
    naruon_lineage: NaruonArchiveInclusionLineageEnvelope,
    private_report: Option<PrivateEvidenceReceipt>,
) -> PublicSummary {
    let mut notices = report.notices.clone();
    notices.extend([
        "public summary omits local paths, archive names, and per-file paths".into(),
        "private evidence is create-new mode 0600 and is not approval".into(),
        "no archive, cloud item, or source file was changed".into(),
    ]);
    PublicSummary {
        schema_kind: "disksage.archive-content-inclusion-batch-summary/v1",
        version: 1,
        generated_at_ms: report.generated_at_ms,
        local_paths_redacted: true,
        archive_names_redacted: true,
        root_mode: report.root_mode.clone(),
        archive_input_count: report.archive_input_count,
        archive_valid_count: report.archive_valid_count,
        issue_count: report.issue_count,
        pair_comparison_count: report.pair_comparison_count,
        inclusion_relation_count: report.inclusion_relation_count,
        strict_inclusion_relation_count: report.strict_inclusion_relation_count,
        identical_relation_count: report.identical_relation_count,
        reclaim_review_candidate_count: report.reclaim_review_candidate_count,
        reclaim_review_compressed_bytes: report.reclaim_review_compressed_bytes,
        evidence_complete: report.evidence_complete,
        batch_fingerprint_sha256: report.batch_fingerprint_sha256.clone(),
        naruon_lineage,
        private_report,
        filesystem_mutation_executed: false,
        deletion_authorized: false,
        notices,
    }
}

fn execute(args: &Args, generated_at_ms: u64) -> Result<PublicSummary, String> {
    let root = canonical_root(&args.root)?;
    let paths = discover_zip_paths(&root, args.max_discovery_entries, args.max_archives)?;
    let root_mode = if args.strip_shared_root {
        ArchiveTreeRootMode::StripSharedRoot
    } else {
        ArchiveTreeRootMode::KeepTopLevel
    };
    let report = audit_zip_content_inclusion_batch(
        &paths,
        root_mode,
        ArchiveContentBatchOptions {
            max_archives: args.max_archives,
            max_pairs: args.max_pairs,
        },
        generated_at_ms,
    )?;
    let naruon_lineage = export_naruon_archive_inclusion_lineage(&report)?;
    let receipt = args
        .private_output
        .as_ref()
        .map(|path| write_private_json_create_new(&root, path, &report))
        .transpose()?;
    Ok(public_summary(&report, naruon_lineage, receipt))
}

fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw)?;
    let generated_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system-time-before-epoch".to_string())?
        .as_millis() as u64;
    let summary = execute(&args, generated_at_ms)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&summary).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        for (name, contents) in entries {
            archive
                .start_file(
                    name,
                    SimpleFileOptions::default().unix_permissions(0o100644),
                )
                .unwrap();
            archive.write_all(contents).unwrap();
        }
        archive.finish().unwrap();
    }

    #[test]
    fn parser_requires_absolute_root_and_bounds_discovery() {
        let parsed = parse_args(&[
            "--root".into(),
            "/tmp/downloads".into(),
            "--private-output".into(),
            "/tmp/private.json".into(),
            "--max-archives".into(),
            "64".into(),
            "--max-pairs".into(),
            "1000".into(),
            "--max-discovery-entries".into(),
            "5000".into(),
            "--strip-shared-root".into(),
        ])
        .unwrap();
        assert_eq!(parsed.max_archives, 64);
        assert_eq!(parsed.max_pairs, 1000);
        assert_eq!(parsed.max_discovery_entries, 5000);
        assert!(parsed.strip_shared_root);
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["--root".into(), "relative".into()]).is_err());
        assert!(parse_args(&[
            "--root".into(),
            "/tmp".into(),
            "--max-discovery-entries".into(),
            (MAX_DISCOVERY_ENTRIES + 1).to_string(),
        ])
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn discovery_is_recursive_bounded_and_does_not_follow_symlinks() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("nested")).unwrap();
        std::fs::write(root.path().join("a.zip"), b"a").unwrap();
        std::fs::write(root.path().join("nested").join("b.ZIP"), b"b").unwrap();
        std::fs::write(outside.path().join("outside.zip"), b"outside").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();

        let paths = discover_zip_paths(root.path(), 100, 10).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|path| path.starts_with(root.path())));
        assert_eq!(
            discover_zip_paths(root.path(), 1, 10).unwrap_err(),
            "archive-batch-discovery-entry-limit"
        );
        assert_eq!(
            discover_zip_paths(root.path(), 100, 1).unwrap_err(),
            "archive-batch-discovery-archive-limit"
        );
    }

    #[cfg(unix)]
    #[test]
    fn execution_writes_private_paths_but_public_summary_is_redacted() {
        let root = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        write_zip(&root.path().join("subset-secret.zip"), &[("a.txt", b"a")]);
        write_zip(
            &root.path().join("superset-secret.zip"),
            &[("a.txt", b"a"), ("b.txt", b"b")],
        );
        let private_path = private.path().join("private.json");
        let args = Args {
            root: root.path().to_path_buf(),
            private_output: Some(private_path.clone()),
            max_archives: 10,
            max_pairs: 10,
            max_discovery_entries: 100,
            strip_shared_root: false,
        };

        let summary = execute(&args, 42).unwrap();
        let public = serde_json::to_string(&summary).unwrap();
        let private = std::fs::read_to_string(private_path).unwrap();

        assert_eq!(summary.inclusion_relation_count, 1);
        assert_eq!(summary.reclaim_review_candidate_count, 1);
        assert!(summary.private_report.is_some());
        assert_eq!(summary.naruon_lineage.relation_count, 1);
        assert!(!summary.naruon_lineage.local_paths_included);
        assert!(!public.contains("subset-secret.zip"));
        assert!(!public.contains(root.path().to_string_lossy().as_ref()));
        assert!(private.contains("subset-secret.zip"));
        assert!(private.contains("superset-secret.zip"));
        assert!(!summary.filesystem_mutation_executed);
        assert!(!summary.deletion_authorized);
    }
}
