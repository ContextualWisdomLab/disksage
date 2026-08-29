use std::path::Path;
use std::process::Command;

fn create_cargo_fixture(root: &Path) {
    let project = root.join("fixture");
    std::fs::create_dir_all(project.join("target")).unwrap();
    std::fs::write(
        project.join("Cargo.toml"),
        b"[package]\nname='fixture'\nversion='0.0.0'\n",
    )
    .unwrap();
    std::fs::write(project.join("Cargo.lock"), b"version = 4\n").unwrap();
    std::fs::write(project.join("target/output.bin"), vec![7_u8; 4096]).unwrap();
}

fn run_cli(root: &Path, extra: &[&str]) -> std::process::Output {
    let binary = env!("CARGO_BIN_EXE_disksage-dev-artifacts");
    let mut command = Command::new(binary);
    command.arg("--root").arg(root).arg("--min-age-days").arg("0");
    command.args(extra);
    command.output().unwrap()
}

#[test]
fn read_only_plan_serializes_exact_selection_bound_approval_phrase() {
    let temp = tempfile::tempdir().unwrap();
    create_cargo_fixture(temp.path());

    let output = run_cli(temp.path(), &[]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["executed"], false);
    assert_eq!(report["candidate_count"], 1);
    let phrase = report["exact_approval_phrase"]
        .as_str()
        .expect("read-only plan must serialize the backend phrase required for execution");
    assert!(phrase.starts_with("MOVE DEVELOPMENT ARTIFACTS "));
    assert!(phrase.ends_with(" TO TRASH"));
}

#[test]
fn execute_flag_alone_cannot_authorize_development_artifact_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    create_cargo_fixture(temp.path());
    let target = temp.path().join("fixture/target");

    let output = run_cli(temp.path(), &["--execute"]);
    assert!(
        !output.status.success(),
        "--execute without the exact reviewed selection phrase must fail closed"
    );
    assert!(target.join("output.bin").is_file());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("development-artifact-confirmation-required"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
