use std::path::PathBuf;

#[path = "../src/bin/disksage-provider-oauth-entry.rs"]
mod provider_oauth_entry;

#[test]
fn windows_home_authority_matches_core_userprofile_contract() {
    let unix_style_home = PathBuf::from("C:/unexpected-home");
    let user_profile = PathBuf::from("C:/Users/DiskSageOperator");

    assert_eq!(
        provider_oauth_entry::environment_home_from(
            Some(unix_style_home.clone()),
            Some(user_profile.clone()),
            true,
        ),
        Some(user_profile),
        "Windows provider OAuth must use the same USERPROFILE home authority as DiskSage core",
    );
    assert_eq!(
        provider_oauth_entry::environment_home_from(Some(unix_style_home), None, true),
        None,
        "Windows must fail closed when its canonical USERPROFILE authority is unavailable",
    );
}

#[test]
fn non_windows_home_authority_remains_home() {
    let home = PathBuf::from("/home/disksage");
    let unrelated_user_profile = PathBuf::from("C:/Users/Foreign");

    assert_eq!(
        provider_oauth_entry::environment_home_from(
            Some(home.clone()),
            Some(unrelated_user_profile),
            false,
        ),
        Some(home),
    );
    assert_eq!(
        provider_oauth_entry::environment_home_from(None, None, false),
        None,
    );
}
