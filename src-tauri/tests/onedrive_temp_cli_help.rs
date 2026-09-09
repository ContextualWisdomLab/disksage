use std::process::Command;

#[test]
fn help_is_a_successful_side_effect_free_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-onedrive-temp-reclaim"))
        .arg("--help")
        .output()
        .expect("OneDrive temp reclaim CLI should start");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("help stdout should be UTF-8"),
        "usage: disksage-onedrive-temp-reclaim [--apply FINGERPRINT APPROVAL_PHRASE]\n"
    );
    assert!(output.stderr.is_empty());
}
