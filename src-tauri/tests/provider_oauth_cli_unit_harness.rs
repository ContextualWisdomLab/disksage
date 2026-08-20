//! Compile the provider OAuth CLI as a test module so its parser and projection tests run in the
//! ordinary Rust test/coverage lane even though the shipped binary is feature-gated.

mod provider_oauth_cli {
    include!("../src/bin/disksage-provider-oauth.rs");

    #[cfg(not(coverage))]
    fn os_strings(values: &[&str]) -> Vec<std::ffi::OsString> {
        values.iter().map(std::ffi::OsString::from).collect()
    }

    #[cfg(not(coverage))]
    #[test]
    fn sole_help_flags_are_successful_terminal_parse_results() {
        let home = Some(implementation::absolute_home());
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

    #[cfg(all(not(coverage), unix))]
    #[test]
    fn non_utf8_host_argument_is_bounded_before_string_argument_iteration() {
        use std::os::unix::ffi::OsStringExt;

        let error = parse_terminal_args(
            &[std::ffi::OsString::from_vec(vec![0xff, b'x'])],
            Some(implementation::absolute_home()),
        )
        .unwrap_err();

        assert_eq!(error, "argument-encoding-invalid");
    }
}
