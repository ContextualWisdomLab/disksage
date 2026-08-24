//! Real provider-OAuth CLI coverage for discovered local provider roots before external work.
//!
//! These tests include the shipped CLI entrypoint and exercise root discovery plus the read-only
//! connection lookup boundary. They deliberately stop before browser, network, keyring, provider
//! mutation, or source eviction authority can be reached.

mod provider_oauth_cli {
    include!("../src/bin/disksage-provider-oauth.rs");

    #[cfg(not(coverage))]
    fn fresh_home(name: &str) -> std::path::PathBuf {
        let home = std::env::temp_dir().join(format!(
            "disksage-provider-oauth-discovered-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("test home should be creatable");
        home
    }

    #[cfg(not(coverage))]
    fn run_missing_connection_case(provider_directory: &str, action: &str) {
        let home = fresh_home(&format!("{}-{action}", provider_directory.replace(' ', "-")));
        let root = home.join(provider_directory);
        std::fs::create_dir(&root).expect("provider root should be creatable");
        let connections = home.join("connections.json");

        let error = implementation::run_with_environment(
            vec![
                action.into(),
                "--cloud-root".into(),
                root.to_string_lossy().into_owned(),
                "--connections".into(),
                connections.to_string_lossy().into_owned(),
            ],
            Some(home.clone()),
        )
        .unwrap_err();

        assert_eq!(error, "provider-oauth-connection-missing");
        assert!(
            !connections.exists(),
            "read-only lookup failure must not create a durable connection document"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[cfg(not(coverage))]
    #[test]
    fn discovered_onedrive_root_reaches_read_only_connection_lookup_before_external_work() {
        for action in ["--disconnect", "--verify-capacity"] {
            run_missing_connection_case("OneDrive", action);
        }
    }

    #[cfg(not(coverage))]
    #[test]
    fn discovered_google_drive_root_reaches_read_only_connection_lookup_before_external_work() {
        for action in ["--disconnect", "--verify-capacity"] {
            run_missing_connection_case("Google Drive", action);
        }
    }

    #[cfg(not(coverage))]
    #[test]
    fn discovered_icloud_root_is_rejected_before_connection_or_external_work() {
        let home = fresh_home("icloud");
        let root = home.join("iCloudDrive");
        std::fs::create_dir(&root).expect("iCloud root should be creatable");
        let connections = home.join("connections.json");

        let error = implementation::run_with_environment(
            vec![
                "--disconnect".into(),
                "--cloud-root".into(),
                root.to_string_lossy().into_owned(),
                "--connections".into(),
                connections.to_string_lossy().into_owned(),
            ],
            Some(home.clone()),
        )
        .unwrap_err();

        assert_eq!(error, "icloud-oauth-not-supported");
        assert!(!connections.exists());
        let _ = std::fs::remove_dir_all(home);
    }
}
