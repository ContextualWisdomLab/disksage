#[cfg(all(unix, not(target_os = "macos")))]
use std::fs;
#[cfg(all(unix, not(target_os = "macos")))]
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
        .args(["--execute", "--purge-proven-cache-trash", "--journal-path"])
        .arg(&journal)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("cache-trash-identity-bound-permanent-delete-unavailable"));
    assert!(
        npm.exists(),
        "fail-closed CLI must preserve the reviewed cache object"
    );
    assert!(
        !journal.exists(),
        "refusal must happen before journal mutation"
    );
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn shipped_cli_honors_xdg_data_home_for_read_only_trash_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let xdg_data_home = temp.path().join("xdg-data");
    let trash = xdg_data_home.join("Trash/files");
    let npm = trash.join("_cacache");
    fs::create_dir_all(npm.join("content-v2")).unwrap();
    fs::create_dir(npm.join("tmp")).unwrap();
    fs::write(npm.join("content-v2/entry"), b"cache").unwrap();

    let default_trash = home.join(".local/share/Trash/files/v11");
    fs::create_dir_all(default_trash.join("metadata")).unwrap();
    fs::create_dir(default_trash.join("metadata-full")).unwrap();

    let journal = temp.path().join("state/journal.jsonl");
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &xdg_data_home)
        .args(["--purge-proven-cache-trash", "--journal-path"])
        .arg(&journal)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let evidence: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let candidates = evidence["proven_cache_trash"].as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["name"], "_cacache");
    assert_eq!(candidates[0]["path"], npm.to_string_lossy().as_ref());
    assert!(
        !candidates.iter().any(|candidate| candidate["name"] == "v11"),
        "custom XDG_DATA_HOME must take precedence over the default home Trash"
    );
    assert!(
        !journal.exists(),
        "read-only evidence collection must not create the journal"
    );
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn shipped_cli_rejects_duplicate_authority_singletons_before_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let journal_a = temp.path().join("state/a.jsonl");
    let journal_b = temp.path().join("state/b.jsonl");

    let duplicate_journal = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .env("HOME", &home)
        .env_remove("XDG_DATA_HOME")
        .arg("--journal-path")
        .arg(&journal_a)
        .arg("--journal-path")
        .arg(&journal_b)
        .output()
        .unwrap();
    assert_eq!(duplicate_journal.status.code(), Some(2));
    assert!(duplicate_journal.stdout.is_empty());
    assert!(String::from_utf8(duplicate_journal.stderr)
        .unwrap()
        .contains("--journal-path may be supplied once"));

    let duplicate_purge = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .env("HOME", &home)
        .env_remove("XDG_DATA_HOME")
        .args(["--purge-proven-cache-trash", "--purge-proven-cache-trash"])
        .output()
        .unwrap();
    assert_eq!(duplicate_purge.status.code(), Some(2));
    assert!(duplicate_purge.stdout.is_empty());
    assert!(String::from_utf8(duplicate_purge.stderr)
        .unwrap()
        .contains("--purge-proven-cache-trash may be supplied once"));

    let duplicate_execute = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .env("HOME", &home)
        .env_remove("XDG_DATA_HOME")
        .args([
            "--execute",
            "--execute",
            "--purge-proven-cache-trash",
        ])
        .output()
        .unwrap();
    assert_eq!(duplicate_execute.status.code(), Some(2));
    assert!(duplicate_execute.stdout.is_empty());
    assert!(String::from_utf8(duplicate_execute.stderr)
        .unwrap()
        .contains("--execute may be supplied once"));

    assert!(!journal_a.exists());
    assert!(!journal_b.exists());
}

#[test]
fn operator_docs_match_the_fail_closed_permanent_delete_contract() {
    let repository_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri must live directly under the repository root");
    let runbook = std::fs::read_to_string(
        repository_root.join("docs/development/cache-cleanup-operator-runbook.md"),
    )
    .unwrap();
    let adr = std::fs::read_to_string(
        repository_root.join(
            "docs/architecture/adr/0002-cache-cleanup-is-per-item-evidence-bound.md",
        ),
    )
    .unwrap();

    for document in [&runbook, &adr] {
        assert!(document.contains(
            "cache-trash-identity-bound-permanent-delete-unavailable"
        ));
        assert!(!document.contains("permanently removes only"));
        assert!(!document.contains("may permanently remove only"));
    }
    assert!(runbook.contains("empty the native Trash manually"));
    assert!(adr.contains("before journal or filesystem mutation"));
}
