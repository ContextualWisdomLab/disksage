use disksage_lib::multipart_archive::{
    collect_multipart_archive_audit, summarize_multipart_audit, DEFAULT_MAX_ENTRIES,
};
use disksage_lib::private_evidence::write_private_json_create_new;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    root: PathBuf,
    max_entries: usize,
    private_output: Option<PathBuf>,
}

fn usage() -> String {
    format!(
        "usage: disksage-multipart-archive-audit --root ABSOLUTE_PATH \
         [--max-entries 1..={DEFAULT_MAX_ENTRIES}] \
         [--private-output ABSOLUTE_NEW_FILE.json]"
    )
}

fn parse_args(raw: &[String]) -> Result<Args, String> {
    let mut root = None;
    let mut max_entries = DEFAULT_MAX_ENTRIES;
    let mut private_output = None;
    let mut index = 0usize;
    while index < raw.len() {
        let value = |index: &mut usize, flag: &str| -> Result<String, String> {
            *index += 1;
            raw.get(*index)
                .cloned()
                .ok_or_else(|| format!("{flag} 값이 필요함"))
        };
        match raw[index].as_str() {
            "--root" => {
                if root.is_some() {
                    return Err("--root는 한 번만 지정할 수 있음".into());
                }
                root = Some(PathBuf::from(value(&mut index, "--root")?));
            }
            "--max-entries" => {
                let parsed = value(&mut index, "--max-entries")?
                    .parse::<usize>()
                    .map_err(|_| "--max-entries는 양의 정수여야 함".to_string())?;
                if parsed == 0 || parsed > DEFAULT_MAX_ENTRIES {
                    return Err(format!(
                        "--max-entries는 1..={DEFAULT_MAX_ENTRIES} 범위여야 함"
                    ));
                }
                max_entries = parsed;
            }
            "--private-output" => {
                if private_output.is_some() {
                    return Err("--private-output은 한 번만 지정할 수 있음".into());
                }
                private_output = Some(PathBuf::from(value(&mut index, "--private-output")?));
            }
            "--help" | "-h" => return Err(usage()),
            _ => return Err("알 수 없는 인자".to_string()),
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
        max_entries,
        private_output,
    })
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
}

fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(not(coverage))]
fn run() -> Result<(), String> {
    let raw = std::env::args().skip(1).collect::<Vec<_>>();
    if raw.len() == 1 && matches!(raw[0].as_str(), "--help" | "-h") {
        println!("{}", usage());
        return Ok(());
    }
    let args = parse_args(&raw)?;
    let report = collect_multipart_archive_audit(&args.root, system_now_ms(), args.max_entries)?;
    let mut summary =
        serde_json::to_value(summarize_multipart_audit(&report)).map_err(|e| e.to_string())?;
    if let Some(path) = &args.private_output {
        let receipt = write_private_json_create_new(&args.root, path, &report)?;
        summary
            .as_object_mut()
            .ok_or_else(|| "multipart audit summary JSON object가 아님".to_string())?
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

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage multipart archive audit: {error}");
        std::process::exit(2);
    }
}

#[cfg(coverage)]
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_absolute_arguments() {
        let args = parse_args(&[
            "--root".into(),
            "/source".into(),
            "--max-entries".into(),
            "42".into(),
            "--private-output".into(),
            "/private/audit.json".into(),
        ])
        .unwrap();
        assert_eq!(args.root, PathBuf::from("/source"));
        assert_eq!(args.max_entries, 42);
        assert_eq!(
            args.private_output,
            Some(PathBuf::from("/private/audit.json"))
        );
    }

    #[test]
    fn rejects_missing_relative_duplicate_and_unbounded_arguments() {
        for raw in [
            vec![],
            vec!["--root".into(), "relative".into()],
            vec!["--root".into(), "/a/../b".into()],
            vec![
                "--max-entries".into(),
                "0".into(),
                "--root".into(),
                "/a".into(),
            ],
            vec![
                "--max-entries".into(),
                (DEFAULT_MAX_ENTRIES + 1).to_string(),
                "--root".into(),
                "/a".into(),
            ],
            vec!["--root".into(), "/a".into(), "--root".into(), "/b".into()],
            vec!["--wat".into()],
        ] {
            assert!(parse_args(&raw).is_err(), "{raw:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn private_output_is_create_new_mode_0600_and_outside_source() {
        use std::os::unix::fs::PermissionsExt;

        let source = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let path = private.path().join("audit.json");
        let value = serde_json::json!({"private": true});
        let receipt = write_private_json_create_new(source.path(), &path, &value).unwrap();
        assert_eq!(receipt.sha256.len(), 64);
        assert!(receipt.bytes > 0);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(write_private_json_create_new(source.path(), &path, &value).is_err());
        assert!(write_private_json_create_new(
            source.path(),
            &source.path().join("inside.json"),
            &value
        )
        .is_err());
    }
}
