use std::fs;
use std::path::PathBuf;

fn brew_cleanup_source() -> String {
    fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/brew_cleanup.rs"))
        .expect("brew cleanup production source must be readable")
}

#[test]
fn verified_brew_snapshot_boundary_is_exercisable_in_unix_tests_only() {
    let source = brew_cleanup_source();
    let testable_unix_cfg = "#[cfg(any(target_os = \"macos\", all(test, unix)))]";

    assert!(
        source.contains(&format!(
            "{testable_unix_cfg}\nstruct VerifiedBrewExecutable"
        )),
        "the verified executable holder must remain macOS production code while becoming exercisable in Unix unit tests"
    );
    assert!(
        source.contains(&format!("{testable_unix_cfg}\nfn open_verified_brew")),
        "the exact executable opener must be testable on the Linux CI runner without broadening runtime platform support"
    );
}

#[test]
fn brew_command_must_use_identity_preserving_cancellable_unix_lifecycle() {
    let source = brew_cleanup_source();
    let run_command = source
        .split_once("fn run_command(mut command: std::process::Command) -> Result<CommandOutput, String> {")
        .expect("brew cleanup run_command boundary must exist")
        .1
        .split_once("fn run_verified_brew(")
        .expect("brew cleanup run_command boundary must end before run_verified_brew")
        .0;

    assert!(
        run_command.contains("wait_for_child_without_reap("),
        "brew cleanup must keep the private process-group leader waitable until descendant cleanup settles"
    );
    assert!(
        run_command.contains("PipeReaderCancellation::new()"),
        "brew cleanup must own explicit cancellation for descendants that retain inherited stdout/stderr writers"
    );
    assert!(
        run_command.matches("spawn_bounded_cancellable_pipe_reader(").count() >= 2,
        "both stdout and stderr must use the canonical bounded cancellable reader lifecycle"
    );
    assert!(
        !run_command.contains("child.try_wait()"),
        "brew cleanup must not reap the leader before the final private-group cleanup opportunity"
    );
    assert!(
        !run_command.contains("thread::spawn(move || read_bounded"),
        "brew cleanup must not retain blocking reader threads that can outlive the command deadline"
    );

    let cancellation = run_command
        .find("reader_cancellation.cancel();")
        .expect("child settlement must publish reader cancellation");
    let stdout_join = run_command[cancellation..]
        .find("stdout_reader")
        .and_then(|offset| run_command[cancellation + offset..].find(".join()"))
        .map(|offset| cancellation + offset)
        .expect("stdout reader must remain owned and joined after cancellation");
    let stderr_join = run_command[cancellation..]
        .find("stderr_reader")
        .and_then(|offset| run_command[cancellation + offset..].find(".join()"))
        .map(|offset| cancellation + offset)
        .expect("stderr reader must remain owned and joined after cancellation");
    assert!(
        cancellation < stdout_join && cancellation < stderr_join,
        "reader cancellation must be visible before either owned pipe reader can be joined"
    );
}
