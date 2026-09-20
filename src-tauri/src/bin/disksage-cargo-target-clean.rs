//! Fail-closed `cargo clean` for a project `target/` with measured reclaim bytes.

use disksage_lib::cargo_target_reclaim::{clean_cargo_target, ledger_reclaim_bytes};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "Usage: disksage-cargo-target-clean --project-dir ABSOLUTE_PATH\n\
Runs cargo clean with an absolute cargo executable. Exit 2 on tool/spawn/nonzero failure.\n\
Prints JSON with bytes_before/after and observed_reduction_bytes (0 when unchanged).";

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let mut project_dir: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-h" | "--help") => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            Some("--project-dir") => {
                let value = match args.next() {
                    Some(v) => PathBuf::from(v),
                    None => {
                        eprintln!("DiskSage cargo-target-clean: --project-dir requires PATH");
                        return ExitCode::from(2);
                    }
                };
                if project_dir.replace(value).is_some() {
                    eprintln!("DiskSage cargo-target-clean: --project-dir may be supplied once");
                    return ExitCode::from(2);
                }
            }
            Some(other) => {
                eprintln!("DiskSage cargo-target-clean: unknown option: {other}\n{USAGE}");
                return ExitCode::from(2);
            }
            None => {
                eprintln!("DiskSage cargo-target-clean: invalid UTF-8 option\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(project_dir) = project_dir else {
        eprintln!("DiskSage cargo-target-clean: --project-dir is required\n{USAGE}");
        return ExitCode::from(2);
    };

    match clean_cargo_target(&project_dir) {
        Ok(result) => {
            let reclaim = ledger_reclaim_bytes(&result);
            println!(
                "{{\n  \"cargo_path\": {:?},\n  \"project_dir\": {:?},\n  \"target_dir\": {:?},\n  \"bytes_before\": {},\n  \"bytes_after\": {},\n  \"observed_reduction_bytes\": {},\n  \"ledger_reclaim_bytes\": {},\n  \"status_code\": {},\n  \"executed\": {}\n}}",
                result.cargo_path,
                result.project_dir,
                result.target_dir,
                result.bytes_before,
                result.bytes_after,
                result.observed_reduction_bytes,
                reclaim,
                result.status_code,
                result.executed
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("DiskSage cargo-target-clean: {error}");
            ExitCode::from(2)
        }
    }
}
