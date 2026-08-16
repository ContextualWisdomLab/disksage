//! Read-only cache cleanup plan. It exposes the same metadata-bound candidates as the GUI.

use disksage_lib::rules::{cache_candidates, BaseDirs};

#[derive(Debug, Default, PartialEq, Eq)]
struct Args {
    id: Option<String>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--id" => {
                index += 1;
                let id = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "--id 값이 필요함".to_string())?;
                if id.is_empty() {
                    return Err("--id 값이 비어 있음".into());
                }
                parsed.id = Some(id);
            }
            "--help" | "-h" => {
                return Err("usage: disksage-clean-plan [--id CACHE_ID]".into());
            }
            unknown => return Err(format!("알 수 없는 인자: {unknown}")),
        }
        index += 1;
    }
    Ok(parsed)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn run(args: &[String]) -> Result<(), String> {
    let parsed = parse_args(args)?;
    let bases = BaseDirs::from_env().ok_or("환경변수에서 기본 경로를 찾지 못함")?;
    let mut candidates = cache_candidates(&bases);
    if let Some(id) = parsed.id {
        candidates.retain(|candidate| candidate.id == id);
    }
    let mut notices = vec![
        "dry-run-only",
        "metadata-fingerprint-only",
        "trash-delete-requires-explicit-review",
    ];
    if candidates.iter().any(|candidate| !candidate.scan_complete) {
        notices.push("metadata-manifest-bounded");
    }
    let payload = serde_json::json!({
        "generated_at_ms": now_ms(),
        "candidates": candidates,
        "notices": notices,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn parser_distinguishes_help_from_invalid_input() {
        assert_eq!(parse_args(&[]).unwrap(), ParseOutcome::Run(Args::default()));
        assert_eq!(
            parse_args(&[OsString::from("--help")]).unwrap(),
            ParseOutcome::Help
        );
        assert_eq!(
            parse_args(&[OsString::from("-h")]).unwrap(),
            ParseOutcome::Help
        );
        assert_eq!(
            parse_args(&[OsString::from("--nope")]).unwrap_err(),
            "알 수 없는 인자"
        );
    }

    #[test]
    fn parser_accepts_optional_utf8_cache_id() {
        assert_eq!(
            parse_args(&[OsString::from("--id"), OsString::from("trivy-cache")]).unwrap(),
            ParseOutcome::Run(Args {
                id: Some("trivy-cache".into())
            })
        );
        assert_eq!(
            parse_args(&[OsString::from("--id")]).unwrap_err(),
            "--id 값이 필요함"
        );
        assert_eq!(
            parse_args(&[OsString::from("--id"), OsString::from("")]).unwrap_err(),
            "--id 값이 비어 있음"
        );
    }

    #[cfg(unix)]
    #[test]
    fn parser_rejects_non_utf8_cache_id_and_redacts_non_utf8_unknown_argument() {
        use std::os::unix::ffi::OsStringExt;

        let non_utf8 = OsString::from_vec(vec![0xff]);
        assert_eq!(
            parse_args(&[OsString::from("--id"), non_utf8]).unwrap_err(),
            "--id 값은 UTF-8이어야 함"
        );
        let unknown = OsString::from_vec(vec![b'-', b'-', 0xff]);
        assert_eq!(parse_args(&[unknown]).unwrap_err(), "알 수 없는 인자");
    }
}
