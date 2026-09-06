use disksage_lib::dev_artifacts::find_artifacts;
use std::fs;
use std::path::Path;

#[test]
fn arbitrary_package_project_does_not_authorize_generic_build_directory() {
    let temp = tempfile::tempdir().expect("temporary test root");
    let build = temp.path().join(".build");

    fs::create_dir_all(&build).expect("generic build directory");
    fs::write(temp.path().join("package.json"), b"{}")
        .expect("ordinary package manifest");
    fs::write(build.join("customer-owned.sqlite"), b"not a disposable tool cache")
        .expect("customer-owned payload");

    let found = find_artifacts(temp.path(), 0, u64::MAX);

    assert!(
        !found.iter().any(|artifact| {
            artifact.kind == ".build" || Path::new(&artifact.path) == build.as_path()
        }),
        "an arbitrary .build directory beside package.json must remain outside destructive development-artifact cleanup authority"
    );
}
