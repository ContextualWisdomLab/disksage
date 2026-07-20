//! Read-only ZIP-to-Git-tree proof. Archive entries are streamed and never extracted.

use std::path::PathBuf;

use disksage_lib::archive_git_tree::inspect_zip_git_tree;

#[derive(Debug, PartialEq, Eq)]
struct Args {
    zip: PathBuf,
    expected_tree: Option<String>,
}

fn usage() -> &'static str {
    "DiskSage archive Git tree proof: usage: disksage-archive-tree --zip PATH [--expected-tree HEX40]"
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
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--zip" => zip = Some(PathBuf::from(value(args, &mut index, "--zip")?)),
            "--expected-tree" => expected_tree = Some(value(args, &mut index, "--expected-tree")?),
            "--help" | "-h" => return Err(usage().into()),
            unknown => return Err(format!("알 수 없는 인자: {unknown}")),
        }
        index += 1;
    }
    Ok(Args {
        zip: zip.ok_or_else(|| "--zip 값이 필요함".to_string())?,
        expected_tree,
    })
}

fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw)?;
    let report = inspect_zip_git_tree(&args.zip, args.expected_tree.as_deref())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
    );
    if report.matches_expected == Some(false) {
        return Err("archive-git-tree-mismatch".into());
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
            ])
            .unwrap(),
            Args {
                zip: PathBuf::from("/tmp/source.zip"),
                expected_tree: Some("a".repeat(40)),
            }
        );
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["--unknown".into()]).is_err());
    }
}
