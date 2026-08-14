use disksage_lib::volume_pressure::{compare_snapshots, snapshot_volume, LocalVolumeSnapshot};
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const MAX_BASELINE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Eq, PartialEq)]
struct Args {
    path: PathBuf,
    baseline: Option<PathBuf>,
    logical_removed_bytes: Option<u64>,
}

fn main() -> ExitCode {
    match run(std::env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error == "help" => {
            println!(
                "Usage: disksage-volume-snapshot [--path PATH] \
                 [--baseline SNAPSHOT_JSON --logical-removed-bytes BYTES]"
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error:{error}");
            ExitCode::from(2)
        }
    }
}

fn run<I, S>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let args = parse_args(args)?;
    let current = snapshot_volume(&args.path, now_ms()?)?;
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    match args.baseline {
        Some(path) => {
            let baseline = read_baseline(path)?;
            let comparison = compare_snapshots(&baseline, &current, args.logical_removed_bytes)?;
            serde_json::to_writer_pretty(&mut output, &comparison)
                .map_err(|_| "local-volume-output-encode-failed")?;
        }
        None => {
            serde_json::to_writer_pretty(&mut output, &current)
                .map_err(|_| "local-volume-output-encode-failed")?;
        }
    }
    writeln!(&mut output).map_err(|_| "local-volume-output-write-failed")?;
    Ok(())
}

fn parse_args<I, S>(args: I) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let values: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if values.len() == 1 && matches!(values[0].to_str(), Some("--help" | "-h")) {
        return Err("help".into());
    }

    let mut path = None;
    let mut baseline = None;
    let mut logical_removed_bytes = None;
    let mut values = values.into_iter();
    while let Some(flag) = values.next() {
        match flag.to_str() {
            Some("--help" | "-h") => return Err("local-volume-argument-unknown".into()),
            Some("--path") => {
                if path.is_some() {
                    return Err("local-volume-path-duplicate".into());
                }
                path = Some(PathBuf::from(
                    values.next().ok_or("local-volume-path-value-missing")?,
                ));
            }
            Some("--baseline") => {
                if baseline.is_some() {
                    return Err("local-volume-baseline-duplicate".into());
                }
                baseline = Some(PathBuf::from(
                    values.next().ok_or("local-volume-baseline-value-missing")?,
                ));
            }
            Some("--logical-removed-bytes") => {
                if logical_removed_bytes.is_some() {
                    return Err("local-volume-logical-removed-duplicate".into());
                }
                let raw_bytes = values
                    .next()
                    .ok_or("local-volume-logical-removed-value-missing")?;
                logical_removed_bytes = Some(
                    raw_bytes
                        .to_str()
                        .ok_or("local-volume-logical-removed-invalid")?
                        .parse::<u64>()
                        .map_err(|_| "local-volume-logical-removed-invalid")?,
                );
            }
            Some(_) | None => return Err("local-volume-argument-unknown".into()),
        }
    }
    if baseline.is_none() && logical_removed_bytes.is_some() {
        return Err("local-volume-logical-removed-requires-baseline".into());
    }
    Ok(Args {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        baseline,
        logical_removed_bytes,
    })
}

fn read_baseline(path: PathBuf) -> Result<LocalVolumeSnapshot, String> {
    let metadata =
        std::fs::symlink_metadata(&path).map_err(|_| "local-volume-baseline-unavailable")?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("local-volume-baseline-not-regular-file".into());
    }
    if metadata.len() > MAX_BASELINE_BYTES {
        return Err("local-volume-baseline-too-large".into());
    }
    let file = File::open(path).map_err(|_| "local-volume-baseline-unavailable")?;
    let mut encoded = Vec::new();
    file.take(MAX_BASELINE_BYTES + 1)
        .read_to_end(&mut encoded)
        .map_err(|_| "local-volume-baseline-read-failed")?;
    if encoded.len() as u64 > MAX_BASELINE_BYTES {
        return Err("local-volume-baseline-too-large".into());
    }
    let snapshot =
        serde_json::from_slice(&encoded).map_err(|_| "local-volume-baseline-json-invalid")?;
    disksage_lib::volume_pressure::validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn now_ms() -> Result<u64, String> {
    let observed_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "local-volume-clock-invalid")?
        .as_millis() as u64;
    if observed_at_ms == 0 {
        return Err("local-volume-clock-invalid".into());
    }
    Ok(observed_at_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use disksage_lib::volume_pressure::snapshot_volume;
    use std::fs;

    #[test]
    fn parser_defaults_to_current_directory() {
        assert_eq!(
            parse_args(Vec::<String>::new()).unwrap(),
            Args {
                path: PathBuf::from("."),
                baseline: None,
                logical_removed_bytes: None,
            }
        );
    }

    #[test]
    fn parser_accepts_bounded_comparison_arguments() {
        assert_eq!(
            parse_args([
                "--path",
                "/volume",
                "--baseline",
                "/tmp/baseline.json",
                "--logical-removed-bytes",
                "123",
            ])
            .unwrap(),
            Args {
                path: PathBuf::from("/volume"),
                baseline: Some(PathBuf::from("/tmp/baseline.json")),
                logical_removed_bytes: Some(123),
            }
        );
    }

    #[test]
    fn parser_rejects_unknown_duplicate_and_unbound_arguments() {
        assert_eq!(
            parse_args(["--unknown"]).unwrap_err(),
            "local-volume-argument-unknown"
        );
        assert_eq!(
            parse_args(["--path", ".", "--path", "."]).unwrap_err(),
            "local-volume-path-duplicate"
        );
        assert_eq!(
            parse_args(["--logical-removed-bytes", "1"]).unwrap_err(),
            "local-volume-logical-removed-requires-baseline"
        );
        assert_eq!(
            parse_args(["--baseline"]).unwrap_err(),
            "local-volume-baseline-value-missing"
        );
    }

    #[test]
    fn baseline_reader_accepts_only_bounded_valid_snapshot_json() {
        let temp = tempfile::tempdir().unwrap();
        let snapshot = snapshot_volume(temp.path(), 1).unwrap();
        let valid = temp.path().join("valid.json");
        fs::write(&valid, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        assert_eq!(read_baseline(valid).unwrap(), snapshot);

        let malformed = temp.path().join("malformed.json");
        fs::write(&malformed, b"not-json").unwrap();
        assert_eq!(
            read_baseline(malformed).unwrap_err(),
            "local-volume-baseline-json-invalid"
        );

        let oversized = temp.path().join("oversized.json");
        fs::write(&oversized, vec![b'x'; MAX_BASELINE_BYTES as usize + 1]).unwrap();
        assert_eq!(
            read_baseline(oversized).unwrap_err(),
            "local-volume-baseline-too-large"
        );
    }

    #[cfg(unix)]
    #[test]
    fn baseline_reader_rejects_symlink() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let snapshot = snapshot_volume(temp.path(), 1).unwrap();
        let valid = temp.path().join("valid.json");
        let linked = temp.path().join("linked.json");
        fs::write(&valid, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        symlink(valid, &linked).unwrap();
        assert_eq!(
            read_baseline(linked).unwrap_err(),
            "local-volume-baseline-not-regular-file"
        );
    }
}
