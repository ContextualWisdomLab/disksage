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
        .find("fn run_brew_object_bound(")
        .map(|offset| runner_start + offset)
        .expect("verified brew runner must end before brew object-bound wrapper");
    let runner = &source[runner_start..runner_end];

    assert!(
        runner.contains(".args([\"-p\", \"-c\","),
        "the fixed bash launcher must preserve Homebrew's privileged-mode shebang behavior and ignore BASH_ENV"
    );
}

#[test]
fn observation_failure_targets_only_the_direct_child_before_reader_settlement() {
    let source = source("src/brew_cleanup.rs");
    let runner_start = source
        .find("fn run_command(")
        .expect("bounded command runner must exist");
    let runner_end = source[runner_start..]
        .find("fn run_verified_brew(")
        .map(|offset| runner_start + offset)
        .expect("bounded command runner must end before verified brew wrapper");
    let runner = &source[runner_start..runner_end];
    let failure_start = runner
        .find("// Without a successful no-reap observation")
        .expect("observation failure branch must document its identity boundary");
    let failure_end = runner[failure_start..]
        .find("\n        }\n    };")
        .map(|offset| failure_start + offset)
        .expect("observation failure branch must end before reader settlement");
    let failure = &runner[failure_start..failure_end];

    assert!(
        failure.contains("child.kill()") && failure.contains("child.wait()"),
        "observation failure must terminate and reap the direct child"
    );
    assert!(
        !failure.contains("signal_private_process_group("),
        "observation failure must never guess a negative process-group ID"
    );
    let settlement = &runner[failure_end..];
    let cancellation = settlement
        .find("reader_cancellation.cancel();")
        .expect("reader settlement must publish cancellation");
    let stdout_join = settlement
        .find("stdout_reader.join()")
        .expect("stdout reader must be joined");
    let stderr_join = settlement
        .find("stderr_reader.join()")
        .expect("stderr reader must be joined");
    assert!(
        cancellation < stdout_join && cancellation < stderr_join,
        "reader cancellation must be visible before both owned joins"
    );
}
