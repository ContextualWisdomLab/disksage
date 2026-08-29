#[cfg(target_os = "linux")]
mod linux_npx_only {
    use std::fs;
    use std::process::Command;

    fn isolated_command(tmp: &tempfile::TempDir) -> Command {
        let home = tmp.path().join("home");
        let cache_home = tmp.path().join("cache");
        let data_home = tmp.path().join("data");
        let temp_root = tmp.path().join("tmp");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&cache_home).unwrap();
        fs::create_dir_all(&data_home).unwrap();
        fs::create_dir_all(&temp_root).unwrap();

        let mut command = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"));
        command
            .env("HOME", home)
            .env("XDG_CACHE_HOME", cache_home)
            .env("XDG_DATA_HOME", data_home)
            .env("TMPDIR", temp_root);
        command
    }

    #[test]
    fn npx_only_execute_is_empty_success_when_cache_root_is_absent() {
        let tmp = tempfile::tempdir().expect("create isolated npx fixture");
        let journal = tmp.path().join("journal.jsonl");
        let output = isolated_command(&tmp)
            .arg("--execute")
            .arg("--npx-only")
            .arg("--journal-path")
            .arg(&journal)
            .output()
            .expect("run production npx-only cleanup binary");

        assert!(
            output.status.success(),
            "an absent npx cache is an empty successful cleanup, not an error: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("parse npx-only receipt");
        assert_eq!(json["executed"], true);
        assert_eq!(json["npx_only"], true);
        assert_eq!(json["results"], serde_json::json!([]));
        assert!(!journal.exists(), "empty cleanup must not fabricate a mutation journal");
    }

    #[test]
    fn npx_only_dry_run_declares_its_scope() {
        let tmp = tempfile::tempdir().expect("create isolated npx dry-run fixture");
        let journal = tmp.path().join("journal.jsonl");
        let output = isolated_command(&tmp)
            .arg("--npx-only")
            .arg("--journal-path")
            .arg(&journal)
            .output()
            .expect("run production npx-only dry run");

        assert!(output.status.success());
        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("parse npx-only dry-run receipt");
        assert_eq!(json["executed"], false);
        assert_eq!(json["npx_only"], true);
        assert!(!journal.exists(), "dry run must not create a mutation journal");
    }

    #[test]
    fn npx_only_execution_moves_environment_to_trash_instead_of_permanent_delete() {
        let tmp = tempfile::tempdir().expect("create isolated npx cleanup fixture");
        let home = tmp.path().join("home");
        let environment = home.join(".npm/_npx/environment-a");
        let journal = tmp.path().join("journal.jsonl");
        fs::create_dir_all(&environment).unwrap();
        fs::write(environment.join("package.json"), b"{\"name\":\"fixture\"}\n").unwrap();

        let output = isolated_command(&tmp)
            .arg("--execute")
            .arg("--npx-only")
            .arg("--journal-path")
            .arg(&journal)
            .output()
            .expect("run production npx-only cleanup binary");

        assert!(
            output.status.success(),
            "npx-only cleanup failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!environment.exists(), "inactive npx environment should be reclaimed");
        let journal_text = fs::read_to_string(&journal).expect("read npx cleanup journal");
        assert!(
            journal_text.contains("\"op\":\"trash_delete\""),
            "npx-only cleanup must preserve the reversible Trash contract: {journal_text}"
        );
        assert!(
            !journal_text.contains("permanent_generated_directory_delete"),
            "npx-only cleanup must not silently grant permanent deletion authority: {journal_text}"
        );
    }
}
