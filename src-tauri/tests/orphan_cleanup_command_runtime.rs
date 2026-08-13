use std::fs;
use std::path::PathBuf;

fn source(path: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join(path)).expect("repository source must be readable")
}

#[test]
fn orphan_cleanup_is_async_and_replans_before_trash() {
    let commands = source("src/commands.rs");
    let start = commands
        .find("pub async fn clean_orphan_candidates(")
        .expect("orphan cleanup command must exist");
    let end = commands[start..]
        .find("pub fn list_cache_candidates(")
        .map(|offset| start + offset)
        .expect("orphan cleanup command must precede cache listing");
    let command = &commands[start..end];
    let attribute_start = commands[..start]
        .rfind("#[tauri::command")
        .expect("orphan cleanup command must have a Tauri command attribute");
    assert!(commands[attribute_start..start].contains("#[tauri::command(async)]"));
    assert!(command.contains("orphan::plan(&home, now_ms())"));
    assert!(command.contains("candidate.auto_trash_eligible"));
    assert!(command.contains("clean_paths_inner"));
}

#[test]
fn orphan_judgment_is_advisory_and_registered() {
    let commands = source("src/commands.rs");
    let start = commands
        .find("pub async fn judge_orphan_cleanup(")
        .expect("relation-aware orphan judgment command must exist");
    let end = commands[start..]
        .find("pub async fn clean_orphan_candidates(")
        .map(|offset| start + offset)
        .expect("orphan judgment command must precede cleanup command");
    let command = &commands[start..end];
    assert!(command.contains("judge_plan"));
    assert!(command.contains("InferenceEngine"));
    let lib = source("src/lib.rs");
    assert!(lib.contains("commands::judge_orphan_cleanup"));
}
