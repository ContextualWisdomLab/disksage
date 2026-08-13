use std::fs;
use std::path::PathBuf;

fn source(path: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join(path)).expect("repository source must be readable")
}

#[test]
fn brew_cleanup_plan_runs_off_the_tauri_main_thread() {
    let commands = source("src/commands.rs");
    let start = commands
        .find("pub fn plan_brew_cleanup()")
        .expect("Homebrew plan command must exist");
    let prefix = &commands[..start];
    let attribute_start = prefix
        .rfind("#[tauri::command")
        .expect("Homebrew plan command must have a Tauri command attribute");
    assert!(
        prefix[attribute_start..].contains("#[tauri::command(async)]"),
        "blocking Homebrew subprocess planning must use Tauri's async command execution context"
    );
}

#[test]
fn brew_cleanup_judgment_releases_engine_before_storing_authority() {
    let commands = source("src/commands.rs");
    let start = commands
        .find("pub fn judge_brew_cleanup(")
        .expect("Homebrew judgment command must exist");
    let end = commands[start..]
        .find("pub fn execute_brew_cleanup(")
        .map(|offset| start + offset)
        .expect("judgment command must precede execution command");
    let judgment = &commands[start..end];
    let infer = judgment
        .find("let judgment = brew_cleanup::judge(engine, &plan, now_ms());")
        .expect("judgment must invoke the local inference engine");
    let release = judgment
        .find("drop(guard);")
        .expect("engine lock must be explicitly released after inference");
    let store = judgment
        .find("brew_cleanup_judgment")
        .expect("safe judgment storage boundary must exist");
    assert!(infer < release);
    assert!(release < store);
}
