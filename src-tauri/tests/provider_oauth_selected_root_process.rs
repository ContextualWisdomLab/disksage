#![cfg(feature = "cloud-cli")]

//! Black-box inheritance of the still-valid selected-root safety evidence from #156.
//!
//! These cases stop before browser, credential-store, or provider-network work. They exercise the
//! shipped CLI after local cloud-root discovery and prove that invalid or unconnected roots fail
//! without creating the durable OAuth connection document.

use std::process::Command;

fn run_provider_oauth(home: &std::path::Path, args: &[&std::ffi::OsStr]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_disksage-provider-oauth"));
    command.env("HOME", home);
    #[cfg(windows)]
    command.env("USERPROFILE", home);
    command
        .args(args)
        .output()
        .expect("provider OAuth CLI should start")
}

#[test]
fn discovered_root_reaches_action_guards_without_external_work_or_durable_mutation() {
    let temp = tempfile::tempdir().expect("temporary home should be created");
    let root = temp.path().join("OneDrive");
    std::fs::create_dir(&root).expect("OneDrive discovery fixture should be created");
    let connections = temp.path().join("private/connections.json");

    let connect = run_provider_oauth(
        temp.path(),
        &[
            "--connect".as_ref(),
            "--cloud-root".as_ref(),
            root.as_os_str(),
            "--client-id".as_ref(),
            " ".as_ref(),
            "--manual-browser".as_ref(),
            "--connections".as_ref(),
            connections.as_os_str(),
        ],
    );
    assert!(!connect.status.success());
    assert!(connect.stdout.is_empty());
    assert_eq!(
        String::from_utf8(connect.stderr).expect("diagnostic should be UTF-8").trim(),
        "oauth-client-id-invalid"
    );

    for action in ["--verify-capacity", "--disconnect"] {
        let output = run_provider_oauth(
            temp.path(),
            &[
                action.as_ref(),
                "--cloud-root".as_ref(),
                root.as_os_str(),
                "--connections".as_ref(),
                connections.as_os_str(),
            ],
        );
        assert!(!output.status.success(), "action: {action}");
        assert!(output.stdout.is_empty(), "action: {action}");
        assert_eq!(
            String::from_utf8(output.stderr)
                .expect("diagnostic should be UTF-8")
                .trim(),
            "provider-oauth-connection-missing",
            "action: {action}"
        );
    }

    assert!(
        !connections.exists(),
        "pre-network failure paths must not create a connection document"
    );
}
