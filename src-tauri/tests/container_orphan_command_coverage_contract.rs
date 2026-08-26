use std::fs;
use std::path::PathBuf;

fn source(path: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join(path)).expect("repository source must be readable")
}

#[test]
fn shipped_container_orphan_commands_remain_present_in_coverage_builds() {
    let command_owner = source("src/container_orphan_commands.rs");
    let lib = source("src/lib.rs");

    for command in ["inspect_container_orphans", "execute_container_orphan_prune"] {
        let signature = format!("pub fn {command}(");
        let start = command_owner
            .find(&signature)
            .unwrap_or_else(|| panic!("shipped Tauri command {command} must exist"));
        let prefix_start = command_owner[..start]
            .rfind("\n///")
            .unwrap_or_else(|| panic!("{command} must retain a documented command boundary"));
        let prefix = &command_owner[prefix_start..start];
        assert!(
            !prefix.contains("#[cfg(not(coverage))]"),
            "coverage builds must retain the shipped {command} command surface"
        );
        assert!(
            prefix.contains("#[tauri::command(async)]"),
            "{command} must remain a Tauri command"
        );
        assert!(
            lib.contains(&format!("container_orphan_commands::{command},")),
            "the production invoke handler must route {command} through its covered owner"
        );
        assert!(
            !lib.contains(&format!("\n            commands::{command},")),
            "the coverage-excluded legacy wrapper must not remain the shipped IPC authority"
        );
    }

    assert!(
        lib.contains("mod container_orphan_commands;"),
        "the covered container-orphan command owner must be compiled with the library"
    );
    assert!(
        command_owner.contains("container_orphan_reclaim")
            && command_owner.contains("podman_reclaim"),
        "container orphan command dependencies must remain available when coverage is enabled"
    );
}
