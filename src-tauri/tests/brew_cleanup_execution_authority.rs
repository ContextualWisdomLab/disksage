use std::fs;
use std::path::PathBuf;

fn source(path: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join(path)).expect("repository source must be readable")
}

#[test]
fn brew_cleanup_execute_must_verify_identity_before_destructive_launch() {
    let source = source("src/brew_cleanup.rs");
    let execute_start = source
        .find("pub fn execute(")
        .expect("brew cleanup execute boundary must exist");
    let execute_end = source[execute_start..]
        .find("const MAX_AUDIT_BYTES")
        .map(|offset| execute_start + offset)
        .expect("execute boundary must end before audit constants");
    let execute = &source[execute_start..execute_end];

    let open = execute
        .find("open_verified_brew(&path)")
        .expect("execute must open and bind the current Homebrew executable before launch");
    let identity_check = execute
        .find("verified.identity != plan.brew_identity")
        .expect("execute must compare the verified executable identity with the authorized plan");
    let destructive_launch = execute
        .find("run_verified_brew(&path, verified, &EXECUTE_ARGUMENTS)")
        .expect("execute must launch only through the already-verified executable handle");

    assert!(
        open < identity_check && identity_check < destructive_launch,
        "identity verification must complete before the destructive Homebrew command starts"
    );
    assert!(
        execute.contains("brew-cleanup-executable-identity-bound-execution-unavailable"),
        "identity mismatch must fail closed"
    );
    assert!(
        !execute.contains("run_brew_object_bound(&path, &EXECUTE_ARGUMENTS)"),
        "execute must not combine destructive launch with a post-launch identity observation"
    );
}

#[test]
fn object_bound_brew_launch_must_use_privileged_bash_mode() {
    let source = source("src/brew_cleanup.rs");
    let runner_start = source
        .find("fn run_verified_brew(")
        .expect("verified brew runner must exist");
    let runner_end = source[runner_start..]
        .find("fn read_bounded(")
        .map(|offset| runner_start + offset)
        .expect("verified brew runner must end before bounded reader");
    let runner = &source[runner_start..runner_end];

    assert!(
        runner.contains(".args([\"-p\", \"-c\","),
        "the fixed bash launcher must preserve Homebrew's privileged-mode shebang behavior and ignore BASH_ENV"
    );
}

#[test]
fn timeout_and_wait_failure_must_not_join_pipe_readers() {
    let source = source("src/brew_cleanup.rs");
    let runner_start = source
        .find("fn run_command(")
        .expect("bounded command runner must exist");
    let runner_end = source[runner_start..]
        .find("fn run_brew_object_bound(")
        .map(|offset| runner_start + offset)
        .expect("bounded command runner must end before brew object-bound wrapper");
    let runner = &source[runner_start..runner_end];
    let timeout_start = runner
        .find("Ok(None) if Instant::now() >= deadline")
        .expect("timeout branch must exist");
    let wait_failure_start = runner
        .find("Err(_) =>")
        .expect("wait-failure branch must exist");
    let timeout = &runner[timeout_start..wait_failure_start];
    let wait_failure = &runner[wait_failure_start..];

    for failure_branch in [timeout, wait_failure] {
        assert!(
            failure_branch.contains("drop(stdout_reader);")
                && failure_branch.contains("drop(stderr_reader);"),
            "failure paths must detach reader threads after terminating the direct child"
        );
        assert!(
            !failure_branch.contains("stdout_reader.join()")
                && !failure_branch.contains("stderr_reader.join()"),
            "failure paths must not wait forever on pipes retained by descendant processes"
        );
    }
}
