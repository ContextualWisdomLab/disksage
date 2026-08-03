//! Read-only ZIP-to-Git-tree proof. Archive entries are streamed and never extracted.

use std::path::PathBuf;

use disksage_lib::archive_git_tree::{
    compare_zip_content_inclusion, inspect_zip_git_tree_with_mode, ArchiveTreeRootMode,
};

#[derive(Debug, PartialEq, Eq)]
struct Args {
    zip: PathBuf,
    expected_tree: Option<String>,
    superset_zip: Option<PathBuf>,
    keep_top_level: bool,
}

fn usage() -> &'static str {
    "DiskSage archive proof: usage: disksage-archive-tree --zip PATH [--expected-tree HEX40 | --prove-subset-of PATH] [--keep-top-level]"
}

fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

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
            unknown => return Err(format!("알 수 없는 인자: {unknown}")),
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

fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
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
