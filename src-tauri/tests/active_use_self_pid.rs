#[cfg(unix)]
#[test]
fn active_use_preserves_current_process_open_handle() {
    let root = tempfile::tempdir().expect("create active-use fixture");
    let payload_path = root.path().join("payload.bin");
    let payload = std::fs::File::create(&payload_path).expect("open cache payload in this process");

    let evidence = disksage_lib::git_worktree::active_use_evidence(root.path(), 5_000, 128, true);

    assert!(evidence.assessed, "active-use probe was not assessed: {evidence:?}");
    assert!(
        evidence.evidence_complete,
        "active-use probe was incomplete: {evidence:?}"
    );
    assert!(evidence.active, "open in-process handle was not retained: {evidence:?}");
    assert!(
        evidence.observed_pids.contains(&std::process::id()),
        "current DiskSage process PID disappeared from lsof evidence: {evidence:?}"
    );

    drop(payload);
}
