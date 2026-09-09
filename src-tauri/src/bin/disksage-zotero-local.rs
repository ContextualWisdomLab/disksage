//! Bounded Zotero Local API metadata importer.

use disksage_lib::zotero_local::{dry_run_summary, parse_manifest, write_references};
use std::ffi::OsString;
use std::io::Read;
use std::path::PathBuf;

const USAGE: &str = "usage: disksage-zotero-local --input ABSOLUTE_JSON [--execute]";

fn read_input(path: &PathBuf) -> Result<Vec<u8>, String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "zotero-input-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("zotero-input-must-be-regular-file".into());
    }
    let file = std::fs::File::open(path).map_err(|_| "zotero-input-unreadable".to_string())?;
    let mut bytes = Vec::new();
    file.take((disksage_lib::zotero_local::MAX_REFERENCE_MANIFEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "zotero-input-read-failed".to_string())?;
    Ok(bytes)
}

fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Option<(PathBuf, bool)>, String> {
    let mut input = None;
    let mut execute = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--input") => {
                let value = args
                    .next()
                    .ok_or_else(|| "--input requires PATH".to_string())?;
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err("--input must be absolute".into());
                }
                if input.replace(path).is_some() {
                    return Err("--input may be supplied once".into());
                }
            }
            Some("--execute") => execute = true,
            Some("--help" | "-h") => return Ok(None),
            Some(value) => return Err(format!("unknown option: {value}")),
            None => return Err("invalid UTF-8 option".into()),
        }
    }
    input
        .map(|path| (path, execute))
        .ok_or_else(|| "--input is required".into())
        .map(Some)
}

fn main() {
    let parsed = match parse_args(std::env::args_os().skip(1)) {
        Ok(Some(parsed)) => parsed,
        Ok(None) => {
            println!("{USAGE}");
            return;
        }
        Err(error) => {
            eprintln!("{USAGE}: {error}");
            std::process::exit(2);
        }
    };
    let bytes = match read_input(&parsed.0) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("disksage-zotero-local: {error}");
            std::process::exit(2);
        }
    };
    let references = match parse_manifest(&bytes) {
        Ok(references) => references,
        Err(error) => {
            eprintln!("disksage-zotero-local: {error}");
            std::process::exit(2);
        }
    };
    if !parsed.1 {
        println!("{}", dry_run_summary(&references));
        return;
    }
    let key = match std::env::var("ZOTERO_LOCAL_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            eprintln!("disksage-zotero-local: ZOTERO_LOCAL_API_KEY is required for --execute");
            std::process::exit(2);
        }
    };
    match write_references(&references, &key) {
        Ok(response) => println!(
            "{}",
            serde_json::json!({
                "executed": true,
                "local_api": disksage_lib::zotero_local::DEFAULT_LOCAL_API_BASE,
                "item_count": references.len(),
                "response": response
            })
        ),
        Err(error) => {
            eprintln!("disksage-zotero-local: {error}");
            std::process::exit(2);
        }
    }
}
