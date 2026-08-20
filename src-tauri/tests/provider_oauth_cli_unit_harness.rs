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
