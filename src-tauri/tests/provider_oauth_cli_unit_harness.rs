//! Compile the provider OAuth CLI as a test module so its parser and projection tests run in the
//! ordinary Rust test/coverage lane even though the shipped binary is feature-gated.

mod provider_oauth_cli {
    include!("../src/bin/disksage-provider-oauth.rs");

    #[cfg(not(coverage))]
    fn os_strings(values: &[&str]) -> Vec<std::ffi::OsString> {
        values.iter().map(std::ffi::OsString::from).collect()
    }

    #[cfg(not(coverage))]
    fn absolute_home() -> std::path::PathBuf {
        std::env::temp_dir().join("disksage-provider-oauth-harness-home")
    }

    #[cfg(not(coverage))]
    fn fresh_home(name: &str) -> std::path::PathBuf {
        let home = std::env::temp_dir().join(format!(
            "disksage-provider-oauth-harness-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("test home should be creatable");
        home
    }

    #[cfg(not(coverage))]
    fn utf8_args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[cfg(not(coverage))]
    #[test]
    fn sole_help_flags_are_successful_terminal_parse_results() {
        let home = Some(absolute_home());
        assert!(matches!(
            parse_terminal_args(&os_strings(&["--help"]), home.clone()).unwrap(),
            TerminalParse::Help
        ));
        assert!(matches!(
            parse_terminal_args(&os_strings(&["-h"]), home.clone()).unwrap(),
            TerminalParse::Help
        ));
        assert!(
            parse_terminal_args(&os_strings(&["--help", "--list"]), home).is_err(),
            "help mixed with an action must remain an invalid request rather than hiding domain work"
        );
    }

    #[cfg(not(coverage))]
    #[test]
    fn windows_home_resolution_uses_userprofile_only_as_a_home_fallback() {
        let home = std::ffi::OsString::from("explicit-home");
        let profile = std::ffi::OsString::from("windows-user-profile");

        assert_eq!(
            environment_home_from(Some(home.clone()), Some(profile.clone()), true),
            Some(std::path::PathBuf::from(home)),
            "HOME remains authoritative when both environment variables exist"
        );
        assert_eq!(
            environment_home_from(None, Some(profile.clone()), true),
            Some(std::path::PathBuf::from(profile.clone())),
            "Windows must accept USERPROFILE when GitHub/desktop process environments omit HOME"
        );
        assert_eq!(
            environment_home_from(None, Some(profile), false),
            None,
            "non-Windows platforms must not silently acquire USERPROFILE semantics"
        );
    }

    #[cfg(not(coverage))]
    #[test]
    fn list_action_runs_without_network_credentials_or_existing_connection_document() {
        let home = fresh_home("list");

        implementation::run_with_environment(utf8_args(&["--list"]), Some(home.clone()))
            .expect("an empty first-run connection list must remain a read-only success");

        let explicit_connections = home.join("explicit-connections.json");
        implementation::run_with_environment(
            vec![
                "--list".into(),
                "--connections".into(),
                explicit_connections.to_string_lossy().into_owned(),
            ],
            Some(home.clone()),
        )
        .expect("an explicit missing connection document must also remain an empty list");

        assert!(
            !explicit_connections.exists(),
            "list authority must never create the connection document"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[cfg(not(coverage))]
    #[test]
    fn parser_rejects_conflicts_relative_paths_and_action_specific_arguments_before_domain_work() {
        let home = fresh_home("parser");
        let absolute_root = home.join("provider-root");
        let absolute_root = absolute_root.to_string_lossy().into_owned();
        let google_client = "1234567890-abcxyz.apps.googleusercontent.com";

        let cases: Vec<(Vec<String>, Option<std::path::PathBuf>, &str)> = vec![
            (vec![], Some(home.clone()), "exactly one action is required"),
            (
                utf8_args(&["--list", "--disconnect"]),
                Some(home.clone()),
                "actions are mutually exclusive",
            ),
            (
                utf8_args(&["--home"]),
                Some(home.clone()),
                "--home requires a value",
            ),
            (
                utf8_args(&["--list", "--home", "relative-home"]),
                None,
                "--home must be absolute",
            ),
            (
                utf8_args(&["--list", "--connections", "relative.json"]),
                Some(home.clone()),
                "--connections must be absolute",
            ),
            (
                utf8_args(&["--disconnect", "--cloud-root", "relative-root"]),
                Some(home.clone()),
                "--cloud-root must be absolute",
            ),
            (
                utf8_args(&["--list", "--manual-browser"]),
                Some(home.clone()),
                "--list does not accept root, client, or browser arguments",
            ),
            (
                vec!["--connect".into(), "--cloud-root".into(), absolute_root.clone()],
                Some(home.clone()),
                "--connect requires --cloud-root and --client-id",
            ),
            (
                vec![
                    "--verify-capacity".into(),
                    "--cloud-root".into(),
                    absolute_root.clone(),
                    "--client-id".into(),
                    google_client.into(),
                ],
                Some(home.clone()),
                "capacity verification and disconnect require only --cloud-root",
            ),
            (
                utf8_args(&[
                    "--list",
                    "--home",
                    "/tmp/disksage-home-a",
                    "--home",
                    "/tmp/disksage-home-b",
                ]),
                None,
                "--home may be supplied once",
            ),
        ];

        for (args, environment_home, expected) in cases {
            assert_eq!(
                implementation::run_with_environment(args, environment_home).unwrap_err(),
                expected
            );
        }

        let _ = std::fs::remove_dir_all(home);
    }

    #[cfg(not(coverage))]
    #[test]
    fn provider_actions_fail_closed_on_an_undiscovered_absolute_root_before_external_work() {
        let home = fresh_home("undiscovered-root");
        let missing_root = home.join("not-a-provider-root");
        let missing_root = missing_root.to_string_lossy().into_owned();
        let google_client = "1234567890-abcxyz.apps.googleusercontent.com";

        for args in [
            vec![
                "--connect".into(),
                "--cloud-root".into(),
                missing_root.clone(),
                "--client-id".into(),
                google_client.into(),
                "--manual-browser".into(),
            ],
            vec![
                "--verify-capacity".into(),
                "--cloud-root".into(),
                missing_root.clone(),
            ],
            vec![
                "--disconnect".into(),
                "--cloud-root".into(),
                missing_root.clone(),
            ],
        ] {
            assert_eq!(
                implementation::run_with_environment(args, Some(home.clone())).unwrap_err(),
                "cloud-root-not-discovered"
            );
        }

        let _ = std::fs::remove_dir_all(home);
    }

    #[cfg(all(not(coverage), unix))]
    #[test]
    fn non_utf8_host_argument_is_bounded_before_string_argument_iteration() {
        use std::os::unix::ffi::OsStringExt;

        let error = parse_terminal_args(
            &[std::ffi::OsString::from_vec(vec![0xff, b'x'])],
            Some(absolute_home()),
        )
        .unwrap_err();

        assert_eq!(error, "argument-encoding-invalid");
    }
}
