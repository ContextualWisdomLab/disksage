use std::fs;
use std::path::PathBuf;

fn source(path: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join(path)).expect("repository source must be readable")
}

#[test]
fn brew_cleanup_execute_must_fail_closed_without_object_bound_executable_launch() {
    let source = source("src/brew_cleanup.rs");
    let execute_start = source
        .find("pub fn execute(")
        .expect("brew cleanup execute boundary must exist");
    let execute_end = source[execute_start..]
        .find("const MAX_AUDIT_BYTES")
        .map(|offset| execute_start + offset)
        .expect("execute boundary must end before audit constants");
    let execute = &source[execute_start..execute_end];

    assert!(
        execute.contains("brew-cleanup-executable-identity-bound-execution-unavailable"),
        "destructive Homebrew cleanup must fail closed until launch remains bound to the exact authorized executable object"
    );
    assert!(
        !execute.contains("run_brew(&path, &EXECUTE_ARGUMENTS)"),
        "execute must not re-resolve a pathname and launch a potentially replaced Homebrew executable"
    );
}
