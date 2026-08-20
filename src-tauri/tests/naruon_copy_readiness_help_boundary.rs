//! Production parser contract for the shipped Naruon readiness verifier terminal boundary.

mod verifier {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/bin/disksage-naruon-copy-readiness-verify.rs"
    ));

    #[test]
    fn sole_help_is_terminal_success_but_mixed_help_remains_invalid() {
        assert!(matches!(
            parse_args(&["--help".into()]),
            Ok(TerminalRequest::Help)
        ));
        assert!(matches!(
            parse_args(&["-h".into()]),
            Ok(TerminalRequest::Help)
        ));
        assert_eq!(
            parse_args(&["--help".into(), "/readiness.json".into()]).unwrap_err(),
            "naruon-copy-readiness-verifier-usage-invalid"
        );
    }

    #[test]
    fn absolute_readiness_path_keeps_verify_authority_separate_from_help() {
        let request = parse_args(&[std::path::Path::new("/readiness.json").as_os_str().into()])
            .expect("absolute readiness path should remain valid");
        match request {
            TerminalRequest::Verify(path) => {
                assert_eq!(path, std::path::Path::new("/readiness.json"));
            }
            TerminalRequest::Help => panic!("an absolute readiness path must not become help"),
        }
    }
}
