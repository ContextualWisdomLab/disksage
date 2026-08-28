use disksage_lib::dev_artifacts::find_artifacts;
use std::fs;
use std::path::Path;

#[test]
fn retained_editor_extension_is_not_reclassified_as_generic_dev_artifact() {
    let temp = tempfile::tempdir().expect("temporary test root");
    let extensions = temp.path().join(".vscode/extensions");
    let obsolete = extensions.join("publisher.old-1.0.0");
    let retained = extensions.join("publisher.keep-1.0.0");

    fs::create_dir_all(&obsolete).expect("obsolete extension directory");
    fs::create_dir_all(retained.join("node_modules/dependency"))
        .expect("retained extension dependency tree");
    fs::write(retained.join("package.json"), b"{}")
        .expect("retained extension manifest");
    fs::write(retained.join("node_modules/dependency/payload.js"), b"generated")
        .expect("retained extension dependency payload");
    fs::write(
        extensions.join(".obsolete"),
        br#"{"publisher.old-1.0.0":true,"publisher.keep-1.0.0":false}"#,
    )
    .expect("native editor lifecycle metadata");

    let found = find_artifacts(temp.path(), 0, u64::MAX);

    assert!(found.iter().any(|artifact| {
        artifact.kind == "vscode-obsolete-extension"
            && artifact.path == obsolete.to_string_lossy()
    }));
    assert!(
        !found
            .iter()
            .any(|artifact| Path::new(&artifact.path).starts_with(&retained)),
        "a retained editor extension must remain outside generic development-artifact cleanup authority"
    );
}
