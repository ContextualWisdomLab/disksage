//! Read-only bundle preview. No move, download, or execution option is exposed.

use std::{ffi::OsString, io::Write, path::Path};

const USAGE: &str = "usage: disksage-organization-plan ABSOLUTE_SOURCE ABSOLUTE_TARGET_PARENT\nPreview only; content meaning and cloud synchronization are not verified.";

fn write_preview(args: &[OsString], output: &mut impl Write) -> Result<(), String> {
    if args.len() == 1 && matches!(args[0].to_str(), Some("--help" | "-h")) {
        return writeln!(output, "{USAGE}").map_err(|error| error.to_string());
    }
    if args.len() != 2 || args.iter().any(|value| value.to_str().is_none()) {
        return Err(USAGE.into());
    }
    let plan = disksage_lib::plan_organization_bundle(Path::new(&args[0]), Path::new(&args[1]))?;
    serde_json::to_writer_pretty(&mut *output, &plan).map_err(|error| error.to_string())?;
    writeln!(output).map_err(|error| error.to_string())
}

fn main() {
    if let Err(error) = write_preview(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &mut std::io::stdout().lock(),
    ) {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_preserves_source_and_refuses_destination_collision() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let source = root.join("document_group");
        let target = root.join("reviewed_groups");
        std::fs::create_dir(&source).unwrap();
        let file_path = source.join("original draft.txt");
        std::fs::write(&file_path, b"distinct original content").unwrap();
        let args = [source.clone().into_os_string(), target.clone().into_os_string()];
        let mut output = Vec::new();
        write_preview(&args, &mut output).unwrap();
        let preview: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(preview["bundle"]["files"][0]["name"], "original draft.txt");
        assert_eq!(preview["classification_source"], serde_json::Value::Null);
        assert!(!target.exists());
        assert_eq!(std::fs::read(&file_path).unwrap(), b"distinct original content");
        std::fs::create_dir_all(target.join("document_group")).unwrap();
        output.clear();
        assert!(write_preview(&args, &mut output).is_err());
        assert!(output.is_empty());
        assert_eq!(std::fs::read(&file_path).unwrap(), b"distinct original content");
        assert!(write_preview(&[], &mut output).is_err());
        assert!(write_preview(&["--execute".into()], &mut output).is_err());
        write_preview(&["--help".into()], &mut output).unwrap();
        assert!(String::from_utf8(output).unwrap().contains("Preview only"));
    }
}
