use disksage_lib::reclaim_protection::{
    artifact_blocking_reason_codes, assess_worktree_protections, parse_orca_terminal_worktree_paths,
    ProtectionContext, REASON_ORCA_TERMINAL_LIVE, REASON_RECENT_WRITES,
};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn orca_live_binding_and_explicit_recent_write_window_block_reclaim() {
    let root = tempfile::tempdir().expect("temporary worktree root");
    let worktree = root.path().join("worktree");
    fs::create_dir_all(&worktree).expect("create worktree");

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs();
    let context = ProtectionContext {
        orca_live_worktree_paths: vec![worktree.clone()],
        recent_write_window_secs: Some(3_600),
        now_unix_secs: Some(now),
        ..ProtectionContext::default()
    };

    let assessment = assess_worktree_protections(
        &worktree,
        None,
        None,
        &context,
        false,
        false,
        Some((false, false)),
        false,
        Some(false),
        false,
    );
    let blockers = artifact_blocking_reason_codes(&assessment);

    assert!(assessment
        .reason_codes
        .iter()
        .any(|code| code == REASON_ORCA_TERMINAL_LIVE));
    assert!(assessment
        .reason_codes
        .iter()
        .any(|code| code == REASON_RECENT_WRITES));
    assert!(blockers.iter().any(|code| code == REASON_ORCA_TERMINAL_LIVE));
    assert!(blockers.iter().any(|code| code == REASON_RECENT_WRITES));
}

#[test]
fn recent_write_protection_has_no_hidden_default_window() {
    let root = tempfile::tempdir().expect("temporary worktree root");
    let worktree = root.path().join("quiet-worktree");
    fs::create_dir_all(&worktree).expect("create worktree");

    let assessment = assess_worktree_protections(
        &worktree,
        None,
        None,
        &ProtectionContext::default(),
        false,
        false,
        Some((false, false)),
        false,
        Some(false),
        false,
    );

    assert!(!assessment
        .reason_codes
        .iter()
        .any(|code| code == REASON_RECENT_WRITES));
}

#[test]
fn orca_terminal_result_wrapper_preserves_worktree_paths() {
    let parsed = parse_orca_terminal_worktree_paths(
        br#"{"result":{"terminals":[{"worktreePath":"/tmp/disksage-a"},{"cwd":"/tmp/disksage-b"}]}}"#,
    )
    .expect("parse Orca terminal evidence");

    assert_eq!(parsed.len(), 2);
    assert!(parsed.iter().any(|path| path.ends_with("disksage-a")));
    assert!(parsed.iter().any(|path| path.ends_with("disksage-b")));
}
