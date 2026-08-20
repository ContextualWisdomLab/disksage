//! Compile the provider OAuth CLI as a test module so its parser and projection tests run in the
//! ordinary Rust test/coverage lane even though the shipped binary is feature-gated.

mod provider_oauth_cli {
    include!("../src/bin/disksage-provider-oauth.rs");

    #[cfg(not(coverage))]
    #[test]
    fn sole_help_flags_are_successful_terminal_parse_results() {
        let home = Some(absolute_home());
        assert!(matches!(
            parse_terminal_args(&strings(&["--help"]), home.clone()).unwrap(),
            TerminalParse::Help
        ));
        assert!(matches!(
            parse_terminal_args(&strings(&["-h"]), home.clone()).unwrap(),
            TerminalParse::Help
        ));
        assert!(
            parse_terminal_args(&strings(&["--help", "--list"]), home).is_err(),
            "help mixed with an action must remain an invalid request rather than hiding domain work"
        );
    }
}
