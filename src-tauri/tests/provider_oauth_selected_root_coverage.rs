#![cfg(feature = "cloud-cli")]

//! Public-boundary coverage for provider OAuth actions after cloud-root discovery succeeds.
//!
//! The cases deliberately stop before browser, credential-store, or provider-network work: an
//! invalid client ID is rejected before authorization starts, while verify/disconnect use an empty
//! local connection document and therefore fail closed before token refresh or capacity calls.

mod provider_oauth_cli {
    include!("../src/bin/disksage-provider-oauth.rs");

    fn fresh_home(name: &str) -> std::path::PathBuf {
        let home = std::env::temp_dir().join(format!(
            "disksage-provider-oauth-selected-root-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("test home should be creatable");
        home
    }

    fn onedrive_root(home: &std::path::Path) -> std::path::PathBuf {
        let root = home.join("OneDrive");
        std::fs::create_dir_all(&root).expect("OneDrive discovery fixture should be creatable");
        root
    }

    #[test]
    fn discovered_root_reaches_oauth_action_guards_without_external_work() {
        let home = fresh_home("action-guards");
        let root = onedrive_root(&home);
        let root_arg = root.to_string_lossy().into_owned();
        let connections = home.join("connections.json");
        let connections_arg = connections.to_string_lossy().into_owned();

        let connect_error = implementation::run_with_environment(
            vec![
                "--connect".into(),
                "--cloud-root".into(),
                root_arg.clone(),
                "--client-id".into(),
                " ".into(),
                "--manual-browser".into(),
                "--connections".into(),
                connections_arg.clone(),
            ],
            Some(home.clone()),
        )
        .expect_err("invalid client IDs must fail before browser or token work");
        assert_eq!(connect_error, "oauth-client-id-invalid");

        for action in ["--verify-capacity", "--disconnect"] {
            let error = implementation::run_with_environment(
                vec![
                    action.into(),
                    "--cloud-root".into(),
                    root_arg.clone(),
                    "--connections".into(),
                    connections_arg.clone(),
                ],
                Some(home.clone()),
            )
            .expect_err("an unconnected discovered root must fail before provider network work");
            assert_eq!(error, "provider-oauth-connection-missing", "action: {action}");
        }

        assert!(
            !connections.exists(),
            "read-only failure paths must not create a connection document"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn discovered_icloud_root_is_rejected_before_oauth_work() {
        let home = fresh_home("icloud");
        let root = home.join("iCloudDrive");
        std::fs::create_dir_all(&root).expect("iCloud discovery fixture should be creatable");
        let root_arg = root.to_string_lossy().into_owned();

        let error = implementation::run_with_environment(
            vec![
                "--disconnect".into(),
                "--cloud-root".into(),
                root_arg,
            ],
            Some(home.clone()),
        )
        .expect_err("iCloud must be rejected as an OAuth provider before credential work");
        assert_eq!(error, "icloud-oauth-not-supported");

        let _ = std::fs::remove_dir_all(home);
    }
}
