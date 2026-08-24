//! Deterministic public-contract coverage for provider-wide sync admission.
//!
//! These tests parse bounded in-memory File Provider evidence or exercise the platform fail-closed
//! boundary. They do not invoke a cloud provider, mutate local files, or retain user paths.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_global_sync::{
    attach_new_copy_admission_notice, inspect_new_copy_admission, parse_dump,
    require_new_copy_admission, ProviderGlobalSyncReport, ProviderGlobalSyncState,
    PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
};

fn report(
    state: ProviderGlobalSyncState,
    evidence_complete: bool,
    blockers: &[&str],
) -> ProviderGlobalSyncReport {
    ProviderGlobalSyncReport {
        schema_version: PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
        provider: CloudProvider::Onedrive,
        evidence_kind: "fileproviderctl-global-dump".into(),
        evidence_complete,
        state,
        upload_progress_present: false,
        download_progress_present: false,
        pending_indexable_count: None,
        blockers: blockers.iter().map(|value| (*value).into()).collect(),
        notices: vec![
            "provider-global-sync-dump-read-only".into(),
            "provider-global-sync-user-paths-not-retained".into(),
        ],
    }
}

#[test]
fn inactive_progress_markers_do_not_invent_pending_work() {
    let dump = r#"
com.microsoft.OneDrive.FileProvider
sync engine state:
    + upload progress: none
    + download progress: (null)
    + pending-indexable-count: 0
    + errors: 0
      i:1 create-item: error:'<nil>'
"#;

    let parsed = parse_dump(CloudProvider::Onedrive, dump).unwrap();
    assert_eq!(parsed.state, ProviderGlobalSyncState::Clear);
    assert!(!parsed.upload_progress_present);
    assert!(!parsed.download_progress_present);
    assert_eq!(parsed.pending_indexable_count, Some(0));
    assert!(parsed.blockers.is_empty());
}

#[test]
fn indexing_and_aggregate_errors_are_independent_blockers() {
    let pending = parse_dump(
        CloudProvider::GoogleDrive,
        "com.google.drivefs.fpext\nsync engine state:\n + indexing: yes\n + pending-indexable-count: 0\n",
    )
    .unwrap();
    assert_eq!(pending.state, ProviderGlobalSyncState::Pending);
    assert_eq!(
        pending.blockers,
        vec!["provider-global-sync-indexing-pending"]
    );

    let error = parse_dump(
        CloudProvider::Onedrive,
        "com.microsoft.OneDrive.FileProvider\nsync engine state:\n + errors: 2\n",
    )
    .unwrap();
    assert_eq!(error.state, ProviderGlobalSyncState::Error);
    assert_eq!(error.blockers, vec!["provider-global-sync-error"]);
}

#[test]
fn explicit_provider_failure_markers_fail_closed() {
    for marker in [
        "temporarily disconnected",
        "user-disabled: yes",
        "can't dump the extension",
        "Error Domain=NSPOSIXErrorDomain Code=5",
    ] {
        let dump = format!(
            "com.microsoft.OneDrive.FileProvider\nsync engine state:\n + {marker}\n"
        );
        let parsed = parse_dump(CloudProvider::Onedrive, &dump).unwrap();
        assert_eq!(parsed.state, ProviderGlobalSyncState::Error);
        assert_eq!(parsed.blockers, vec!["provider-global-sync-error"]);
    }
}

#[test]
fn admission_requires_complete_clear_unblocked_evidence() {
    assert_eq!(
        require_new_copy_admission(&report(ProviderGlobalSyncState::Clear, false, &[]))
            .unwrap_err(),
        "provider-global-sync-evidence-incomplete"
    );
    assert_eq!(
        require_new_copy_admission(&report(ProviderGlobalSyncState::Pending, true, &[]))
            .unwrap_err(),
        "provider-global-sync-pending"
    );
    assert_eq!(
        require_new_copy_admission(&report(
            ProviderGlobalSyncState::Error,
            true,
            &["provider-global-sync-error", "provider-global-sync-transfer-active"],
        ))
        .unwrap_err(),
        "provider-global-sync-error,provider-global-sync-transfer-active"
    );
    assert!(require_new_copy_admission(&report(
        ProviderGlobalSyncState::Clear,
        true,
        &[],
    ))
    .is_ok());
    assert_eq!(ProviderGlobalSyncState::Unavailable.as_str(), "unavailable");
}

#[test]
fn state_labels_and_parser_edge_markers_are_covered() {
    assert_eq!(ProviderGlobalSyncState::Clear.as_str(), "clear");
    assert_eq!(ProviderGlobalSyncState::Pending.as_str(), "pending");
    assert_eq!(ProviderGlobalSyncState::Error.as_str(), "error");

    let progress = parse_dump(
        CloudProvider::GoogleDrive,
        "com.google.drivefs.fpext\nsync engine state:\n + upload progress: queued\n + download progress: active\n",
    )
    .unwrap();
    assert_eq!(progress.state, ProviderGlobalSyncState::Pending);
    assert!(progress.upload_progress_present);
    assert!(progress.download_progress_present);
    assert_eq!(
        progress.blockers,
        vec!["provider-global-sync-transfer-active"]
    );

    let max_pending = parse_dump(
        CloudProvider::Onedrive,
        "com.microsoft.OneDrive.FileProvider\nsync engine state:\n + pending-indexable-count: 2\n + pending-indexable-count: 9\n + needs-indexing: yes\n + errors: not-a-number\n",
    )
    .unwrap();
    assert_eq!(max_pending.pending_indexable_count, Some(9));
    assert_eq!(max_pending.state, ProviderGlobalSyncState::Pending);
    assert_eq!(
        max_pending.blockers,
        vec!["provider-global-sync-indexing-pending"]
    );

    let explicit_error = parse_dump(
        CloudProvider::Onedrive,
        "com.microsoft.OneDrive.FileProvider\nsync engine state:\n + error:'provider failure'\n",
    )
    .unwrap();
    assert_eq!(explicit_error.state, ProviderGlobalSyncState::Error);
}

#[test]
fn invalid_dump_shapes_follow_fail_closed_contract() {
    assert_eq!(
        parse_dump(
            CloudProvider::GoogleDrive,
            "com.microsoft.OneDrive.FileProvider\nsync engine state:\n",
        )
        .unwrap_err(),
        "provider-global-sync-dump-incomplete"
    );
    assert_eq!(
        parse_dump(
            CloudProvider::Onedrive,
            "com.microsoft.OneDrive.FileProvider\nno sync summary here\n",
        )
        .unwrap_err(),
        "provider-global-sync-dump-incomplete"
    );
    assert_eq!(
        parse_dump(CloudProvider::Icloud, "sync engine state:\n").unwrap_err(),
        "provider-global-sync-icloud-specialized"
    );
}

#[test]
fn clear_state_with_a_blocker_remains_blocked_in_notice_and_admission() {
    let mut contradictory = report(
        ProviderGlobalSyncState::Clear,
        true,
        &["provider-global-sync-indexing-pending"],
    );
    assert_eq!(
        require_new_copy_admission(&contradictory).unwrap_err(),
        "provider-global-sync-indexing-pending"
    );

    contradictory.notices.push("keep-this-notice".into());
    let report_snapshot = contradictory.clone();
    attach_new_copy_admission_notice(&mut contradictory.notices, Some(&report_snapshot));
    assert!(contradictory
        .notices
        .contains(&"provider-global-sync-blocked".to_string()));
    assert!(contradictory
        .notices
        .contains(&"keep-this-notice".to_string()));
    assert!(!contradictory
        .notices
        .contains(&"provider-global-sync-clear".to_string()));
}

#[test]
fn notice_projection_replaces_only_provider_global_sync_state() {
    let baseline = vec![
        "keep-this-notice".to_string(),
        "provider-global-sync-clear".to_string(),
        "provider-global-sync-blocked".to_string(),
        "provider-global-sync-evidence-unavailable".to_string(),
    ];

    let mut notices = baseline.clone();
    let clear = report(ProviderGlobalSyncState::Clear, true, &[]);
    attach_new_copy_admission_notice(&mut notices, Some(&clear));
    assert_eq!(notices, ["keep-this-notice", "provider-global-sync-clear"]);

    let mut notices = baseline.clone();
    let blocked = report(
        ProviderGlobalSyncState::Pending,
        true,
        &["provider-global-sync-indexing-pending"],
    );
    attach_new_copy_admission_notice(&mut notices, Some(&blocked));
    assert_eq!(notices, ["keep-this-notice", "provider-global-sync-blocked"]);

    let mut notices = baseline;
    attach_new_copy_admission_notice(&mut notices, None);
    assert_eq!(
        notices,
        ["keep-this-notice", "provider-global-sync-evidence-unavailable"]
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn non_macos_runtime_fails_closed_without_spawning_provider_tools() {
    assert_eq!(
        inspect_new_copy_admission(CloudProvider::GoogleDrive).unwrap_err(),
        "provider-global-sync-unsupported-platform-google-drive"
    );
}
