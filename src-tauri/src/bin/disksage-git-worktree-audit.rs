//! Read-only Git worktree audit. No prune/remove operation is exposed.

use std::path::PathBuf;

#[derive(Debug, Default, PartialEq, Eq)]
struct Args {
    repository: Option<PathBuf>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--repo" => {
                index += 1;
                parsed.repository = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--repo 값이 필요함".to_string())?,
                ));
            }
            "--help" | "-h" => {
                return Err("usage: disksage-git-worktree-audit [--repo PATH]".into());
            }
            unknown => return Err(format!("알 수 없는 인자: {unknown}")),
        }
        index += 1;
    }
    Ok(parsed)
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&raw) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let repository = args
        .repository
        .unwrap_or_else(|| std::env::current_dir().expect("현재 디렉터리를 확인할 수 없습니다"));
    let report =
        match disksage_lib::worktrees::audit(&repository, disksage_lib::worktrees::system_now_ms())
        {
            Ok(report) => report,
            Err(error) => {
                eprintln!("DiskSage Git worktree 감사 실패: {error}");
                std::process::exit(2);
            }
        };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("worktree report serialization failed")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_accepts_optional_repository() {
        assert_eq!(parse_args(&[]).unwrap(), Args::default());
        assert_eq!(
            parse_args(&["--repo".into(), "/repo".into()]).unwrap(),
            Args {
                repository: Some(PathBuf::from("/repo"))
            }
        );
        assert!(parse_args(&["--repo".into()]).is_err());
        assert!(parse_args(&["--unknown".into()]).is_err());
    }
}
