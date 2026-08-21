//! Regression for the File Provider dump success-path process-group boundary.
//!
//! A File Provider helper can outlive the `fileproviderctl` leader while retaining the inherited
//! stdout descriptor. The successful leader-exit arm must therefore terminate the private process
//! group before the production code joins the stdout reader; otherwise a normal provider probe can
//! block indefinitely waiting for EOF.

#[test]
fn successful_provider_dump_terminates_private_group_before_reader_join() {
    let source = include_str!("../src/provider_global_sync.rs");
    let run_dump = source
        .split_once("fn run_dump(provider: CloudProvider) -> Result<String, String> {")
        .expect("provider global-sync run_dump boundary must exist")
        .1
        .split_once("pub fn inspect_new_copy_admission")
        .expect("provider global-sync run_dump boundary must end before public admission")
        .0;

    assert!(
        run_dump.contains(
            "Ok(Some(status)) => {\n                kill_group();\n                break status;\n            }"
        ),
        "successful fileproviderctl exit must terminate the private process group before joining stdout"
    );

    let success_arm = run_dump
        .find("Ok(Some(status)) => {")
        .expect("successful child-exit arm must exist");
    let group_kill = run_dump[success_arm..]
        .find("kill_group();")
        .map(|offset| success_arm + offset)
        .expect("successful child-exit arm must kill its private process group");
    let reader_join = run_dump
        .find("let bytes = reader")
        .expect("reader join boundary must exist");
    assert!(
        group_kill < reader_join,
        "private process group must be terminated before the stdout reader is joined"
    );
}
