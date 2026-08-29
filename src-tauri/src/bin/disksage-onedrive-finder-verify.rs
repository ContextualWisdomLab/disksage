//! Headless OneDrive Finder postcheck entry point.
//!
//! This binary is intentionally path-redacted at stdout. It loads the immutable private Finder
//! assistance receipt produced by DiskSage, re-discovers the selected OneDrive root, and delegates
//! identity/allocation verification to the library before emitting a bounded summary.

use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    cloud_root: PathBuf,
    receipt: PathBuf,
    record_dir: PathBuf,
}

fn parse_args_os(_args: &[OsString]) -> Result<Args, String> {
    Err("onedrive-finder-verification-not-implemented".into())
}

fn main() {
    if let Err(error) = parse_args_os(&std::env::args_os().skip(1).collect::<Vec<_>>()) {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    const CLOUD_ROOT: &str = "/cloud";
    #[cfg(windows)]
    const CLOUD_ROOT: &str = r"C:\cloud";
    #[cfg(not(windows))]
    const RECEIPT: &str = "/records/receipt.finder-assistance.json";
    #[cfg(windows)]
    const RECEIPT: &str = r"C:\records\receipt.finder-assistance.json";
    #[cfg(not(windows))]
    const RECORD_DIR: &str = "/records";
    #[cfg(windows)]
    const RECORD_DIR: &str = r"C:\records";

    #[test]
    fn parser_accepts_explicit_verification_contract() {
        let args = [
            OsString::from("--cloud-root"),
            OsString::from(CLOUD_ROOT),
            OsString::from("--receipt"),
            OsString::from(RECEIPT),
            OsString::from("--record-dir"),
            OsString::from(RECORD_DIR),
        ];
        let parsed = parse_args_os(&args).unwrap();
        assert_eq!(parsed.cloud_root, PathBuf::from(CLOUD_ROOT));
        assert_eq!(parsed.receipt, PathBuf::from(RECEIPT));
        assert_eq!(parsed.record_dir, PathBuf::from(RECORD_DIR));
    }
}
