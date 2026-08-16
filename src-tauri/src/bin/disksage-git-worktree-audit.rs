//! Read-only Git worktree audit. No prune/remove operation is exposed.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

const USAGE: &str = "usage: disksage-git-worktree-audit [--repo PATH]";

#[derive(Debug, Default, PartialEq, Eq)]
struct Args {
    repository: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
enum ParseOutcome {
    Run(Args),
    Help,
}

fn parse_args(args: &[OsString]) -> Result<ParseOutcome, String> {
    let mut parsed = Args::default();
    let mut index = 0usize;
    while index < args.len() {
        let argument = args[index].as_os_str();
        if argument == OsStr::new("--repo") {
            index += 1;
            parsed.repository = Some(PathBuf::from(
                args.get(index)
                    .ok_or_else(|| "--repo 값이 필요함".to_string())?,
            ));
        } else if argument == OsStr::new("--help") || argument == OsStr::new("-h") {
            return Ok(ParseOutcome::Help);
        } else {
            return Err("알 수 없는 인자".into());
        }
        index += 1;
    }
    Ok(ParseOutcome::Run(parsed))
}

fn main() {
    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();
    let args = match parse_args(&raw) {
        Ok(ParseOutcome::Run(args)) => args,
        Ok(ParseOutcome::Help) => {
            println!("{USAGE}");
            return;
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let repository = match args.repository {
        Some(repository) => repository,
        None => match std::env::current_dir() {
            Ok(repository) => repository,
            Err(_) => {
                eprintln!("현재 디렉터리를 확인할 수 없습니다");
                std::process::exit(2);
            }
        },
    };
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
            parse_args(&[OsString::from("--unknown")]).unwrap_err(),
            "알 수 없는 인자"
        );
    }

    #[test]
    fn parser_accepts_optional_repository() {
        assert_eq!(
            parse_args(&[OsString::from("--repo"), OsString::from("/repo")]).unwrap(),
            ParseOutcome::Run(Args {
                repository: Some(PathBuf::from("/repo"))
            })
        );
        assert_eq!(
            parse_args(&[OsString::from("--repo")]).unwrap_err(),
            "--repo 값이 필요함"
        );
    }

    #[cfg(unix)]
    #[test]
    fn parser_preserves_non_utf8_repository_paths_and_redacts_unknown_arguments() {
        use std::os::unix::ffi::OsStringExt;

        let repository = OsString::from_vec(vec![b'/', b'r', b'e', b'p', b'o', 0xff]);
        assert_eq!(
            parse_args(&[OsString::from("--repo"), repository.clone()]).unwrap(),
            ParseOutcome::Run(Args {
                repository: Some(PathBuf::from(repository))
            })
        );
        let unknown = OsString::from_vec(vec![b'-', b'-', 0xff]);
        assert_eq!(
            parse_args(&[unknown]).unwrap_err(),
            "알 수 없는 인자"
        );
    }
}
