use std::fs;
use std::process::Command;

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn shipped_cli_refuses_path_recursive_permanent_cache_trash_deletion() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let trash = home.join(".local/share/Trash/files");
    let npm = trash.join("_cacache");
    fs::create_dir_all(npm.join("content-v2")).unwrap();
    fs::create_dir(npm.join("tmp")).unwrap();
    fs::write(npm.join("content-v2/entry"), b"cache").unwrap();
    let journal = temp.path().join("state/journal.jsonl");

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .env("HOME", &home)
        .env_remove("XDG_DATA_HOME")
        .args([
            "--execute",
            "--purge-proven-cache-trash",
            "--journal-path",
        ])
        .arg(&journal)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("cache-trash-identity-bound-permanent-delete-unavailable"));
    assert!(npm.exists(), "fail-closed CLI must preserve the reviewed cache object");
    assert!(!journal.exists(), "refusal must happen before journal mutation");
}
