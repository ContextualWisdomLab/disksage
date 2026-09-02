use std::process::Command;

#[test]
fn disabled_permanent_purge_creates_no_journal_state() {
    let temp = tempfile::tempdir().expect("temporary test root");
    let approval = temp.path().join("approved.json");
    std::fs::write(&approval, b"[]").expect("approval fixture");

    let journal_parent = temp.path().join("audit");
    let journal = journal_parent.join("journal.jsonl");
    assert!(!journal_parent.exists());

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .arg("--execute")
        .arg("--purge-proven-cache-trash")
        .arg("--approved-cache-trash-candidates")
        .arg(&approval)
        .arg("--journal-path")
        .arg(&journal)
        .env("HOME", temp.path())
        .env("USERPROFILE", temp.path())
        .output()
        .expect("run shipped cache-cleanup CLI");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "disksage-cache-cleanup: cache-trash-permanent-purge-disabled-until-race-safe"
    );
    assert!(
        !journal_parent.exists(),
        "a disabled irreversible command must fail before creating journal directories"
    );
}
