//! Regression contract for the native cloud-copy local-capacity authority.
//!
//! A native File Provider copy stages under the destination parent, so the mutation-time
//! headroom probe must be bound to that destination filesystem rather than to the source volume.

#[test]
fn native_copy_headroom_is_bound_to_the_destination_staging_volume() {
    let commands = include_str!("../src/commands.rs");
    let start = commands
        .find("fn require_local_copy_headroom")
        .expect("native copy headroom gate must remain explicit");
    let tail = &commands[start..];
    let end = tail
        .find("\n}\n\n#[cfg(not(coverage))]\n#[tauri::command(async)]")
        .expect("headroom helper must remain bounded before the next command");
    let helper = &tail[..end];

    assert!(
        helper.contains("candidate.dst"),
        "headroom must be measured on the destination/staging filesystem"
    );
    assert!(
        !helper.contains("candidate.src"),
        "source-volume free space must not authorize or veto destination staging"
    );
}

#[test]
fn cloud_plan_preview_headroom_uses_destination_staging_volume() {
    let cloud = include_str!("../src/cloud.rs");
    let start = cloud
        .find("let mut destination_headroom_insufficient")
        .expect("cloud preview must evaluate destination headroom");
    let tail = &cloud[start..];
    let end = tail
        .find("\n    if !snapshot.source_scan_complete")
        .expect("destination preview gate must remain before source-scan notices");
    let helper = &tail[..end];

    assert!(helper.contains("require_destination_copy_headroom"));
    assert!(helper.contains("candidate.dst"));
    assert!(!helper.contains("candidate.src"));
}
