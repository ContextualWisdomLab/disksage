use disksage_lib::dev_artifacts::find_artifacts;
use std::fs;

#[test]
fn setup_py_project_with_requirements_lock_remains_discoverable() {
    let temp = tempfile::tempdir().expect("create fixture root");
    let project = temp.path().join("legacy-python-app");
    let venv = project.join(".venv");
    fs::create_dir_all(&venv).expect("create generated environment");
    fs::write(project.join("setup.py"), b"from setuptools import setup\nsetup()\n")
        .expect("write recognized Python marker");
    fs::write(project.join("requirements.txt"), b"pytest==8.4.2\n")
        .expect("write recognized rebuild lock input");
    fs::write(venv.join("generated.bin"), vec![0x5a; 4096])
        .expect("write generated environment payload");

    let found = find_artifacts(temp.path(), 0, u64::MAX);

    assert_eq!(found.len(), 1, "a recognized Python marker plus lock input must authorize discovery");
    assert_eq!(found[0].kind, ".venv");
    assert_eq!(found[0].project, "legacy-python-app");
    assert!(found[0].scan_complete);
    assert!(found[0].allocated_bytes > 0);
}
