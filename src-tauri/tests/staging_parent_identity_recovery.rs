use disksage_lib::dev_artifacts::{clean_artifacts, find_artifacts};
use disksage_lib::filesystem_object_id;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_millis() as u64
}

#[test]
fn public_recovery_does_not_complete_against_a_replaced_source_parent() {
    let fixture = tempfile::tempdir().expect("create filesystem fixture");
    let scan_root = fixture.path().join("scan-root");
    let project = scan_root.join("project");
    let victim = project.join(".codegraph");
    std::fs::create_dir_all(&victim).expect("create reviewed regenerable directory");
    std::fs::write(victim.join("payload.bin"), b"reviewed-object")
        .expect("write reviewed payload");

    let observed_at = now_ms();
    let candidates = find_artifacts(&scan_root, 0, observed_at);
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.path == victim.to_string_lossy())
        .expect("public inventory must discover the reviewed .codegraph candidate")
        .clone();
    assert!(candidate.scan_complete);
    assert_eq!(candidate.skipped, 0);

    let source_parent_object_id =
        filesystem_object_id(&project).expect("capture reviewed source-parent identity");
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
        "source_parent_object_id": source_parent_object_id,
        "error": "simulated interruption after OS Trash mutation"
    });
    let journal = fixture.path().join("journal.jsonl");
    let receipt = serde_json::json!({
        "ts_ms": observed_at,
        "op": "trash_delete",
        "path": candidate.path,
        "bytes": candidate.bytes,
        "outcome": format!(
            "mutated_cleanup_pending:{}",
            serde_json::to_string(&recovery).expect("serialize recovery receipt")
        )
    });
    std::fs::write(
        &journal,
        format!(
            "{}\n",
            serde_json::to_string(&receipt).expect("serialize journal receipt")
        ),
    )
    .expect("write durable recovery fixture");

    // Move the reviewed parent, including the actual empty staging residue, away from the path
    // captured during review and replace that parent with a different filesystem object. A retry
    // must not treat "staging name absent under the replacement parent" as completed recovery.
    let reviewed_parent = scan_root.join("project-reviewed");
    std::fs::rename(&project, &reviewed_parent).expect("move reviewed parent object aside");
    assert_eq!(
        filesystem_object_id(&reviewed_parent).expect("read moved reviewed source-parent identity"),
        source_parent_object_id,
        "the staging residue must move with the exact source-parent object that was reviewed"
    );
    std::fs::create_dir(&project).expect("create replacement source parent");
    assert_ne!(
        filesystem_object_id(&project).expect("read replacement source-parent identity"),
        source_parent_object_id
    );
    let retained_staging = reviewed_parent.join(&staging_name);
    assert!(retained_staging.is_dir());

    let results = clean_artifacts(
        std::slice::from_ref(&candidate),
        &scan_root,
        0,
        &journal,
        observed_at.saturating_add(1),
    );

    assert_eq!(results.len(), 1);
    assert!(
        !results[0].ok,
        "recovery must fail closed when the source parent object changed; it must not publish completion against a replacement parent"
    );
    assert!(
        retained_staging.is_dir(),
        "the actual staging residue remains under the reviewed parent and must not be falsely declared cleaned"
    );
    assert_eq!(
        filesystem_object_id(&trashed_surrogate).expect("recovery must not touch the mutated object"),
        candidate.object_id
    );

    let outcomes = std::fs::read_to_string(&journal)
        .expect("read recovery journal")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("parse recovery receipt"))
        .filter_map(|entry| {
            entry
                .get("outcome")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .collect::<Vec<_>>();
    assert!(
        outcomes
            .iter()
            .all(|outcome| !outcome.starts_with("mutated_cleanup_complete:")),
        "a replacement parent must never turn an unresolved staging residue into a terminal completion receipt"
    );
}
