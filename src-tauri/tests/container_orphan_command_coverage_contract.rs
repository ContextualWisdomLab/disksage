use std::fs;
use std::path::PathBuf;

fn commands_source() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join("src/commands.rs")).expect("commands.rs must be readable")
}

#[test]
fn shipped_container_orphan_commands_remain_present_in_coverage_builds() {
    let source = commands_source();

    for command in ["inspect_container_orphans", "execute_container_orphan_prune"] {
        let signature = format!("pub fn {command}(");
        let start = source
            .find(&signature)
            .unwrap_or_else(|| panic!("shipped Tauri command {command} must exist"));
        let prefix_start = source[..start]
            .rfind("\n///")
            .unwrap_or_else(|| panic!("{command} must retain a documented command boundary"));
        let prefix = &source[prefix_start..start];
        assert!(
            !prefix.contains("#[cfg(not(coverage))]"),
            "coverage builds must execute the shipped {command} command surface instead of compiling it out"
        );
        assert!(
            prefix.contains("#[tauri::command(async)]"),
            "{command} must remain a Tauri command"
        );
    }

    assert!(
        source.contains("use crate::container_orphan_reclaim;"),
        "container orphan command dependencies must remain available when coverage is enabled"
    );
}
