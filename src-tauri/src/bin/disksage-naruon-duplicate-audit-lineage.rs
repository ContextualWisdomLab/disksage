//! Export a verified private duplicate-audit report as path-free Naruon lineage JSON.

use disksage_lib::duplicate_audit::DuplicateAuditReport;
use disksage_lib::naruon_duplicate_audit_lineage::export_naruon_duplicate_audit_lineage;
use std::path::{Component, Path, PathBuf};

const MAX_PRIVATE_REPORT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
struct Cli {
    private_report: PathBuf,
    exported_at_ms: u64,
}

fn usage() -> &'static str {
    "usage: disksage-naruon-duplicate-audit-lineage \\
--private-report <absolute-private-report.json> --exported-at-ms <positive-u64>"
}

fn parse_args_from<I>(args: I) -> Result<Cli, String>
where
    I: IntoIterator<Item = String>,
{
    let mut private_report = None;
    let mut exported_at_ms = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--private-report" => {
                if private_report.is_some() {
                    return Err("duplicate-private-report-argument".into());
                }
                private_report = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| "missing-private-report".to_string())?,
                ));
            }
            "--exported-at-ms" => {
                if exported_at_ms.is_some() {
                    return Err("duplicate-exported-at-argument".into());
                }
                exported_at_ms = Some(
                    args.next()
                        .ok_or_else(|| "missing-exported-at".to_string())?
                        .parse::<u64>()
                        .map_err(|_| "invalid-exported-at".to_string())?,
                );
            }
            "--help" | "-h" => return Err(usage().into()),
            _ => return Err(format!("unexpected-argument:{arg}")),
        }
    }
    let private_report = private_report.ok_or_else(|| "missing-private-report".to_string())?;
    if !private_report.is_absolute()
        || private_report
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("private-report-path-unsafe".into());
    }
    let exported_at_ms = exported_at_ms.ok_or_else(|| "missing-exported-at".to_string())?;
    if exported_at_ms == 0 {
        return Err("invalid-exported-at".into());
    }
    Ok(Cli {
        private_report,
        exported_at_ms,
    })
}

fn read_private_report(path: &Path) -> Result<DuplicateAuditReport, String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "private-report-unavailable".to_string())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > MAX_PRIVATE_REPORT_BYTES
    {
        return Err("private-report-unsafe-or-unbounded".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("private-report-permissions-too-broad".into());
        }
    }
    let bytes = std::fs::read(path).map_err(|_| "private-report-read-failed".to_string())?;
    serde_json::from_slice(&bytes).map_err(|_| "private-report-json-invalid".to_string())
}

fn run(cli: Cli) -> Result<String, String> {
    let report = read_private_report(&cli.private_report)?;
    let envelope = export_naruon_duplicate_audit_lineage(&report, cli.exported_at_ms)?;
    serde_json::to_string_pretty(&envelope)
        .map_err(|_| "naruon-duplicate-audit-lineage-json-failed".to_string())
}

fn main() {
    match parse_args_from(std::env::args().skip(1)).and_then(run) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("{error}");
            if error.starts_with("missing-")
                || error.starts_with("invalid-")
                || error.starts_with("unexpected-")
                || error == usage()
            {
                eprintln!("{}", usage());
            }
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_explicit_arguments() {
        let cli = parse_args_from([
            "--private-report".into(),
            "/private/report.json".into(),
            "--exported-at-ms".into(),
            "101".into(),
        ])
        .unwrap();
        assert_eq!(cli.private_report, PathBuf::from("/private/report.json"));
        assert_eq!(cli.exported_at_ms, 101);
    }

    #[test]
    fn rejects_relative_or_zero_arguments() {
        assert_eq!(
            parse_args_from([
                "--private-report".into(),
                "report.json".into(),
                "--exported-at-ms".into(),
                "101".into(),
            ])
            .unwrap_err(),
            "private-report-path-unsafe"
        );
        assert_eq!(
            parse_args_from([
                "--private-report".into(),
                "/private/report.json".into(),
                "--exported-at-ms".into(),
                "0".into(),
            ])
            .unwrap_err(),
            "invalid-exported-at"
        );
    }
}
