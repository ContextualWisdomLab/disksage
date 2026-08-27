use disksage_lib::duplicate_audit::{
    collect_exact_duplicate_audit, exact_duplicate_audit_integrity_valid,
    summarize_exact_duplicate_audit, DEFAULT_MAX_ENTRIES, DEFAULT_MIN_BYTES, MAX_ENTRIES,
};
use disksage_lib::private_evidence::write_private_json_create_new;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    root: PathBuf,
    min_bytes: u64,
    max_entries: usize,
    private_output: Option<PathBuf>,
}

fn usage() -> String {
    format!(
        "usage: disksage-duplicate-audit --root ABSOLUTE_PATH \
         [--min-bytes POSITIVE_INTEGER] [--max-entries 1..={MAX_ENTRIES}] \
         [--private-output ABSOLUTE_NEW_FILE.json]"
    )
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
}

fn native_value(raw: &[OsString], index: &mut usize, flag: &str) -> Result<OsString, String> {
    *index += 1;
    raw.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

fn text_value(raw: &[OsString], index: &mut usize, flag: &str) -> Result<String, String> {
    native_value(raw, index, flag)?
        .into_string()
        .map_err(|_| "duplicate-audit-argument-invalid".to_string())
}

fn parse_args_os(raw: &[OsString]) -> Result<Args, String> {
    let mut root = None;
    let mut min_bytes = DEFAULT_MIN_BYTES;
    let mut min_bytes_seen = false;
    let mut max_entries = DEFAULT_MAX_ENTRIES;
    let mut max_entries_seen = false;
    let mut private_output = None;
    let mut index = 0usize;
    while index < raw.len() {
        match raw[index].to_str() {
            Some("--root") => {
                if root.is_some() {
                    return Err("--root는 한 번만 지정할 수 있음".into());
                }
                root = Some(PathBuf::from(native_value(raw, &mut index, "--root")?));
            }
            Some("--min-bytes") => {
                if min_bytes_seen {
                    return Err("--min-bytes는 한 번만 지정할 수 있음".into());
                }
                min_bytes_seen = true;
                let parsed = text_value(raw, &mut index, "--min-bytes")?
                    .parse::<u64>()
                    .map_err(|_| "--min-bytes는 양의 정수여야 함".to_string())?;
                if parsed == 0 {
                    return Err("--min-bytes는 양의 정수여야 함".into());
                }
                min_bytes = parsed;
            }
            Some("--max-entries") => {
                if max_entries_seen {
                    return Err("--max-entries는 한 번만 지정할 수 있음".into());
                }
                max_entries_seen = true;
                let parsed = text_value(raw, &mut index, "--max-entries")?
                    .parse::<usize>()
                    .map_err(|_| "--max-entries는 양의 정수여야 함".to_string())?;
                if !(1..=MAX_ENTRIES).contains(&parsed) {
                    return Err(format!("--max-entries는 1..={MAX_ENTRIES} 범위여야 함"));
                }
                max_entries = parsed;
            }
            Some("--private-output") => {
                if private_output.is_some() {
                    return Err("--private-output은 한 번만 지정할 수 있음".into());
                }
                private_output = Some(PathBuf::from(native_value(
                    raw,
                    &mut index,
                    "--private-output",
                )?));
            }
            Some("--help" | "-h") => return Err(usage()),
            Some(_) => return Err("알 수 없는 인자".into()),
            None => return Err("duplicate-audit-argument-invalid".into()),
        }
        index += 1;
    }
    let root = root.ok_or_else(|| "--root가 필요함".to_string())?;
    if !absolute_without_parent(&root) {
        return Err("--root는 상위 탐색이 없는 절대 경로여야 함".into());
    }
    if let Some(path) = &private_output {
        if !absolute_without_parent(path) {
            return Err("--private-output은 상위 탐색이 없는 절대 경로여야 함".into());
        }
    }
    Ok(Args {
        root,
        min_bytes,
        max_entries,
        private_output,
    })
}

#[cfg(test)]
fn parse_args(raw: &[String]) -> Result<Args, String> {
    let native: Vec<OsString> = raw.iter().map(OsString::from).collect();
    parse_args_os(&native)
}

fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn run() -> Result<(), String> {
    let raw = std::env::args_os().skip(1).collect::<Vec<_>>();
    if raw.len() == 1 && matches!(raw[0].to_str(), Some("--help" | "-h")) {
        println!("{}", usage());
        return Ok(());
    }
    let args = parse_args_os(&raw)?;
    let report = collect_exact_duplicate_audit(
        &args.root,
        system_now_ms(),
        args.min_bytes,
        args.max_entries,
    )?;
    if !exact_duplicate_audit_integrity_valid(&report) {
        return Err("duplicate-audit-integrity-invalid".into());
    }
    let mut summary = serde_json::to_value(summarize_exact_duplicate_audit(&report))
        .map_err(|error| error.to_string())?;
    if let Some(path) = &args.private_output {
        let receipt = write_private_json_create_new(&args.root, path, &report)?;
        summary
            .as_object_mut()
            .ok_or_else(|| "duplicate audit summary JSON object가 아님".to_string())?
            .insert(
                "private_output".into(),
                serde_json::to_value(receipt).map_err(|error| error.to_string())?,
            );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&summary).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage exact duplicate audit: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_absolute_arguments() {
        let args = parse_args(&[
            "--root".into(),
            "/source".into(),
            "--min-bytes".into(),
            "4096".into(),
            "--max-entries".into(),
            "42".into(),
            "--private-output".into(),
            "/private/duplicates.json".into(),
        ])
        .unwrap();
        assert_eq!(args.root, PathBuf::from("/source"));
        assert_eq!(args.min_bytes, 4096);
        assert_eq!(args.max_entries, 42);
        assert_eq!(
            args.private_output,
            Some(PathBuf::from("/private/duplicates.json"))
        );
    }

    #[test]
    fn rejects_relative_duplicate_missing_and_unbounded_arguments() {
        for raw in [
            vec![],
            vec!["--root".into(), "relative".into()],
            vec!["--root".into(), "/a/../b".into()],
            vec!["--root".into(), "/a".into(), "--root".into(), "/b".into()],
            vec![
                "--root".into(),
                "/a".into(),
                "--min-bytes".into(),
                "0".into(),
            ],
            vec![
                "--root".into(),
                "/a".into(),
                "--max-entries".into(),
                (MAX_ENTRIES + 1).to_string(),
            ],
            vec![
                "--root".into(),
                "/a".into(),
                "--min-bytes".into(),
                "1".into(),
                "--min-bytes".into(),
                "2".into(),
            ],
            vec![
                "--root".into(),
                "/a".into(),
                "--max-entries".into(),
                "1".into(),
                "--max-entries".into(),
                "2".into(),
            ],
            vec![
                "--root".into(),
                "/a".into(),
                "--private-output".into(),
                "relative.json".into(),
            ],
            vec!["--wat".into()],
        ] {
            assert!(parse_args(&raw).is_err(), "{raw:?}");
        }
    }

    #[test]
    fn unknown_argument_does_not_echo_sensitive_value() {
        let secret = "secret-token-user-pasted-by-mistake";
        let error = parse_args(&[secret.into()]).unwrap_err();
        assert_eq!(error, "알 수 없는 인자");
        assert!(!error.contains(secret));
    }

    #[test]
    fn help_discloses_read_only_private_evidence_interface() {
        let help = parse_args(&["--help".into()]).unwrap_err();
        assert!(help.contains("usage: disksage-duplicate-audit"));
        assert!(help.contains("--private-output ABSOLUTE_NEW_FILE.json"));
    }
}
