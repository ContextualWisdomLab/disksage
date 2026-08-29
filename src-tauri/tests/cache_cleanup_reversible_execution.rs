#[cfg(target_os = "linux")]
#[test]
fn automatic_pip_cleanup_stays_reversible_without_permanent_consent() {
    use std::fs;
    use std::process::Command;

    let tmp = tempfile::tempdir().expect("create isolated cache-cleanup fixture");
    let home = tmp.path().join("home");
    let cache_home = tmp.path().join("cache");
    let data_home = tmp.path().join("data");
    let temp_root = tmp.path().join("tmp");
    let pip_archive = cache_home.join("pip/http-v2/archive");
    let journal = tmp.path().join("journal.jsonl");

    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&data_home).unwrap();
    fs::create_dir_all(&temp_root).unwrap();
    fs::create_dir_all(&pip_archive).unwrap();
    fs::write(pip_archive.join("response.body"), b"regenerable pip cache").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .env("HOME", &home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("TMPDIR", &temp_root)
        .arg("--execute")
        .arg("--journal-path")
        .arg(&journal)
        .output()
        .expect("run production cache-cleanup binary");

    assert!(
        output.status.success(),
        "cache cleanup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!pip_archive.exists(), "approved cache archive should be reclaimed");

    let journal_text = fs::read_to_string(&journal).expect("read cleanup journal");
    assert!(
        journal_text.contains("\"op\":\"trash_delete\""),
        "automatic cleanup must retain the reversible Trash contract: {journal_text}"
    );
    assert!(
        !journal_text.contains("permanent_generated_directory_delete"),
        "automatic cleanup must not grant irreversible deletion authority without explicit consent: {journal_text}"
    );
}
