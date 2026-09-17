use disksage_lib::dev_artifacts::{clean_artifacts, find_artifacts};
use disksage_lib::filesystem_object_id;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_millis() as u64
}

#[test]
fn public_cleanup_recovers_post_mutation_staging_residue() {
    let fixture = tempfile::tempdir().expect("create filesystem fixture");
    let project = fixture.path().join("project");
    let victim = project.join(".codegraph");
    std::fs::create_dir_all(&victim).expect("create reviewed regenerable directory");
    std::fs::write(victim.join("payload.bin"), b"reviewed-object")
        .expect("write reviewed payload");

    let observed_at = now_ms();
    let candidates = find_artifacts(fixture.path(), 0, observed_at);
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.path == victim.to_string_lossy())
        .expect("public inventory must discover the reviewed .codegraph candidate")
        .clone();
    assert!(candidate.scan_complete);
    assert_eq!(candidate.skipped, 0);

    // Reproduce the durable state left after the reviewed object reached OS Trash but the process
    // stopped before the private staging directory could be removed. The surrogate move keeps the
    // real filesystem identity observable without deleting unrelated host Trash contents.
    let staging_name = format!(
        ".disksage-trash-{}-{}-{}",
        std::process::id(),
        observed_at,
        0
    );
    let staging_dir = project.join(&staging_name);
    std::fs::create_dir(&staging_dir).expect("create DiskSage staging directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging_dir, std::fs::Permissions::from_mode(0o700))
            .expect("set private staging permissions");
    }
    let staging_object_id =
        filesystem_object_id(&staging_dir).expect("capture staging directory identity");
    let staged = staging_dir.join(".codegraph");
    std::fs::rename(&victim, &staged).expect("simulate production staging move");
    let trashed_surrogate = fixture.path().join("os-trash-surrogate");
    std::fs::rename(&staged, &trashed_surrogate)
        .expect("simulate successful OS Trash mutation while retaining object identity");
    assert_eq!(
        filesystem_object_id(&trashed_surrogate).expect("read surrogate identity"),
        candidate.object_id
    );
    assert!(std::fs::read_dir(&staging_dir)
        .expect("read empty staging directory")
        .next()
        .is_none());

    let recovery = serde_json::json!({
        "staging_name": staging_name,
        "staging_object_id": staging_object_id,
        "target_object_id": candidate.object_id,
        "catalog_root_object_id": serde_json::Value::Null,
        "error": "simulated interruption after OS Trash mutation"
    });
    let outcome = format!(
        "mutated_cleanup_pending:{}",
        serde_json::to_string(&recovery).expect("serialize recovery receipt")
    );
    let journal = fixture.path().join("journal.jsonl");
    let receipt = serde_json::json!({
        "ts_ms": observed_at,
        "op": "trash_delete",
        "path": candidate.path,
        "bytes": candidate.bytes,
        "outcome": outcome
    });
    std::fs::write(
        &journal,
        format!(
            "{}\n",
            serde_json::to_string(&receipt).expect("serialize journal receipt")
        ),
    )
    .expect("write durable recovery fixture");

    let results = clean_artifacts(
        std::slice::from_ref(&candidate),
        fixture.path(),
        0,
        &journal,
        observed_at.saturating_add(1),
    );

    assert_eq!(results.len(), 1);
    assert!(
        results[0].ok,
        "the shipped cleanup boundary must consume an exact pending recovery before requiring a fresh live candidate: {}",
        results[0].error
    );
    assert!(
        !staging_dir.exists(),
        "successful public recovery must remove the verified-empty DiskSage staging directory"
    );
    assert_eq!(
        filesystem_object_id(&trashed_surrogate).expect("recovery must not touch the mutated object"),
        candidate.object_id
    );

    let last_outcome = std::fs::read_to_string(&journal)
        .expect("read recovery journal")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .last()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("parse recovery receipt"))
        .and_then(|entry| entry.get("outcome").and_then(serde_json::Value::as_str).map(str::to_owned))
        .expect("terminal recovery outcome");
    assert!(
        last_outcome.starts_with("mutated_cleanup_complete:"),
        "public recovery must append a self-correlating completion receipt"
    );
}
