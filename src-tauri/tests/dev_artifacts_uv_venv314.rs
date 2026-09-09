use disksage_lib::dev_artifacts::find_artifacts;

#[test]
fn discovers_uv_python_314_environment_in_bare_repository() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join(".git"), "gitdir: /private/fixture").expect("git marker");
    let environment = tmp.path().join(".venv314");
    std::fs::create_dir(&environment).expect("environment directory");
    std::fs::write(
        environment.join("pyvenv.cfg"),
        "home = /opt/python\nversion_info = 3.14.1\n",
    )
    .expect("uv pyvenv metadata");

    let artifacts = find_artifacts(tmp.path(), 0, u64::MAX);

    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].kind, ".venv314");
    assert_eq!(artifacts[0].path, environment.to_string_lossy());
}
