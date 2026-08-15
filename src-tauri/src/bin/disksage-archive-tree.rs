//! Read-only ZIP-to-Git-tree proof.
//!
//! Archive entries are streamed and never extracted. The command computes deterministic Git-tree
//! evidence, optionally verifies one expected tree or a content-subset relation, and never mutates
//! either archive or the local filesystem.

use std::path::PathBuf;

use disksage_lib::archive_git_tree::{
    compare_zip_content_inclusion, inspect_zip_git_tree_with_mode, ArchiveTreeRootMode,
};

/// Parsed arguments for one archive-tree inspection or subset proof.
#[derive(Debug, PartialEq, Eq)]
struct Args {
    /// ZIP archive whose content tree will be inspected.
    zip: PathBuf,
    /// Optional expected 40-character Git tree identifier.
    expected_tree: Option<String>,
    /// Optional archive that must contain every content item from `zip`.
    superset_zip: Option<PathBuf>,
    /// Whether to retain a shared archive root directory in the computed tree.
    keep_top_level: bool,
}

/// Returns the stable command synopsis used by help and bounded validation failures.
fn usage() -> &'static str {
    "DiskSage archive proof: usage: disksage-archive-tree --zip PATH [--expected-tree HEX40 | --prove-subset-of PATH] [--keep-top-level]"
}

/// Returns the required value after one known option and advances the parser index.
fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

/// Parses bounded UTF-8 command arguments without reflecting unknown payloads.
fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut zip = None;
    let mut expected_tree = None;
    let mut superset_zip = None;
    let mut keep_top_level = false;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--zip" => zip = Some(PathBuf::from(value(args, &mut index, "--zip")?)),
            "--expected-tree" => expected_tree = Some(value(args, &mut index, "--expected-tree")?),
            "--prove-subset-of" => {
                superset_zip = Some(PathBuf::from(value(args, &mut index, "--prove-subset-of")?))
            }
            "--keep-top-level" => keep_top_level = true,
            "--help" | "-h" => return Err(usage().into()),
            _ => return Err("archive-tree-unknown-argument".into()),
        }
        index += 1;
    }
    if expected_tree.is_some() && superset_zip.is_some() {
        return Err("--expected-tree와 --prove-subset-of는 함께 사용할 수 없음".into());
    }
    Ok(Args {
        zip: zip.ok_or_else(|| "--zip 값이 필요함".to_string())?,
        expected_tree,
        superset_zip,
        keep_top_level,
    })
}

/// Reads process arguments, performs the requested read-only proof, and prints JSON evidence.
fn run() -> Result<(), String> {
    let raw = std::env::args_os()
        .skip(1)
        .map(|argument| {
            argument
                .into_string()
                .map_err(|_| "archive-tree-argument-invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if raw.len() == 1 && matches!(raw[0].as_str(), "--help" | "-h") {
        println!("{}", usage());
        return Ok(());
    }
    let args = parse_args(&raw)?;
    let root_mode = if args.keep_top_level {
        ArchiveTreeRootMode::KeepTopLevel
    } else {
        ArchiveTreeRootMode::StripSharedRoot
    };
    if let Some(superset_zip) = args.superset_zip {
        let report = compare_zip_content_inclusion(&args.zip, &superset_zip, root_mode)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
        );
        if !report.subset_content_included {
            return Err("archive-content-not-included".into());
        }
    } else {
        let report =
            inspect_zip_git_tree_with_mode(&args.zip, args.expected_tree.as_deref(), root_mode)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
        );
        if report.matches_expected == Some(false) {
            return Err("archive-git-tree-mismatch".into());
        }
    }
    Ok(())
}

/// Runs the CLI and returns exit code 2 for bounded validation or proof failures.
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_requires_one_zip_and_accepts_optional_tree() {
        assert_eq!(
            parse_args(&[
                "--zip".into(),
                "/tmp/source.zip".into(),
                "--expected-tree".into(),
                "a".repeat(40),
                "--keep-top-level".into(),
            ])
            .unwrap(),
            Args {
                zip: PathBuf::from("/tmp/source.zip"),
                expected_tree: Some("a".repeat(40)),
                superset_zip: None,
                keep_top_level: true,
            }
        );
        assert_eq!(
            parse_args(&["--zip".into(), "/tmp/source.zip".into()]).unwrap(),
            Args {
                zip: PathBuf::from("/tmp/source.zip"),
                expected_tree: None,
                superset_zip: None,
                keep_top_level: false,
            }
        );
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["--unknown".into()]).is_err());
    }

    #[test]
    fn parser_accepts_explicit_content_subset_proof() {
        let parsed = parse_args(&[
            "--zip".into(),
            "/tmp/subset.zip".into(),
            "--prove-subset-of".into(),
            "/tmp/superset.zip".into(),
            "--keep-top-level".into(),
        ])
        .unwrap();

        assert_eq!(
            parsed.superset_zip,
            Some(PathBuf::from("/tmp/superset.zip"))
        );
        assert!(parsed.keep_top_level);
        assert!(parse_args(&[
            "--zip".into(),
            "/tmp/subset.zip".into(),
            "--prove-subset-of".into(),
            "/tmp/superset.zip".into(),
            "--expected-tree".into(),
            "a".repeat(40),
        ])
        .is_err());
    }
}
