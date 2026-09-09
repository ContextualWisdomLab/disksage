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
