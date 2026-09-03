use disksage_lib::{bind_retained_ontology_class, filesystem_object_id};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const USAGE: &str = "Usage: disksage-protect-path --path ABSOLUTE_PATH --class RETAINED_CLASS_IRI";

fn bind_current_object(
    path: &Path,
    class_id: &str,
    before_bind: impl FnOnce(),
) -> Result<String, String> {
    let object_id = filesystem_object_id(path)
        .map_err(|_| "ontology-protection-target-unavailable".to_string())?;
    let current_object_id = filesystem_object_id(path)
        .map_err(|_| "ontology-protection-target-changed".to_string())?;
    if current_object_id != object_id {
        return Err("ontology-protection-target-changed".into());
    }
    before_bind();
    bind_retained_ontology_class(path, class_id)?;
    let bound_object_id = filesystem_object_id(path)
        .map_err(|_| "ontology-protection-target-changed".to_string())?;
    if bound_object_id != object_id {
        return Err("ontology-protection-target-changed".into());
    }
    Ok(object_id)
}

fn run(args: impl IntoIterator<Item = OsString>) -> Result<serde_json::Value, String> {
    let args = args.into_iter().collect::<Vec<_>>();
    if args.len() == 1 && matches!(args[0].to_str(), Some("-h" | "--help")) {
        return Ok(serde_json::json!({"help": USAGE}));
    }
    let mut path = None;
    let mut class_id = None;
    let mut index = 0;
    while index < args.len() {
        let value = args
            .get(index + 1)
            .cloned()
            .ok_or_else(|| "ontology-protection-missing-value".to_string())?;
        match args[index].to_str() {
            Some("--path") if path.is_none() => path = Some(PathBuf::from(value)),
            Some("--class") if class_id.is_none() => {
                class_id = Some(
                    value
                        .into_string()
                        .map_err(|_| "ontology-protection-class-invalid".to_string())?,
                )
            }
            _ => return Err("ontology-protection-invalid-argument".into()),
        }
        index += 2;
    }
    let path = path.ok_or_else(|| "ontology-protection-path-required".to_string())?;
    let class_id = class_id.ok_or_else(|| "ontology-protection-class-required".to_string())?;
    let object_id = bind_current_object(&path, &class_id, || {})?;
    Ok(serde_json::json!({
        "schema_kind": "disksage.ontology-protection-binding/v1",
        "class_id": class_id,
        "target_object_id": object_id,
        "path_redacted": true,
        "binding_written": true
    }))
}

fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 1 && matches!(args[0].to_str(), Some("-h" | "--help")) {
        println!("{USAGE}");
        return;
    }
    match run(args) {
        Ok(report) => println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        ),
        Err(error) => {
            eprintln!("disksage-protect-path: {error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RETAINED_CLASS: &str =
        "https://disksage.app/ontology#CustomerRelationshipManagementData";

    #[test]
    fn retained_binding_protects_exact_file_without_exposing_its_path() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("business.db");
        std::fs::write(&file, b"business").unwrap();
        let report = run([
            OsString::from("--path"),
            file.clone().into_os_string(),
            OsString::from("--class"),
            OsString::from(RETAINED_CLASS),
        ])
        .unwrap();

        assert_eq!(report["path_redacted"], true);
        assert!(disksage_lib::is_protected(&file));
        assert!(!report.to_string().contains(file.to_str().unwrap()));

        let installer = temp.path().join("installer.dmg");
        std::fs::write(&installer, b"installer").unwrap();
        assert_eq!(
            bind_retained_ontology_class(&installer, "https://disksage.app/ontology#Installer")
                .unwrap_err(),
            "ontology-protection-class-not-retained"
        );
        assert!(!disksage_lib::is_protected(&installer));
    }

    #[cfg(unix)]
    #[test]
    fn replacement_between_identity_and_binding_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("business.db");
        std::fs::write(&file, b"original").unwrap();
        let original_id = filesystem_object_id(&file).unwrap();

        let error = bind_current_object(&file, RETAINED_CLASS, || {
            std::fs::remove_file(&file).unwrap();
            std::fs::write(&file, b"replacement").unwrap();
        })
        .unwrap_err();

        assert_eq!(error, "ontology-protection-target-changed");
        assert_ne!(filesystem_object_id(&file).unwrap(), original_id);
        assert!(
            !disksage_lib::is_protected(&file),
            "a replacement that was never reviewed must not inherit the failed protection request"
        );
    }
}
