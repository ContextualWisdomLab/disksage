use std::fs;
use std::path::PathBuf;

fn source(path: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join(path)).expect("repository source must be readable")
}

#[test]
fn brew_cleanup_nonzero_exit_is_not_presented_as_completed() {
    let ui = source("../src/lib/BrewCleanup.svelte");

    assert!(
        ui.contains("실행 실패 (종료 코드"),
        "a non-zero Homebrew exit must be announced as a failed execution"
    );
    assert!(
        !ui.contains("execution.executed ? `실행 완료 (종료 코드 ${execution.status_code})`"),
        "the UI must not label every executed command as completed regardless of exit status"
    );
}
