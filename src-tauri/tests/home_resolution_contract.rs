#[path = "../src/home_resolution.rs"]
mod home_resolution;

use std::path::PathBuf;

fn absolute_fixture(name: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!(r"C:\{name}"))
    } else {
        PathBuf::from(format!("/{name}"))
    }
}

#[test]
fn home_resolution_skips_relative_candidates() {
    let expected = absolute_fixture("users/disksage");
    let resolved = home_resolution::select_absolute_home([
        Some(PathBuf::from("relative-app-home")),
        Some(PathBuf::from("relative-home-env")),
        Some(expected.clone()),
    ])
    .expect("an absolute candidate should be selected");

    assert_eq!(resolved, expected);
    assert!(resolved.is_absolute());
}

#[test]
fn home_resolution_fails_closed_when_every_candidate_is_relative_or_missing() {
    let error = home_resolution::select_absolute_home([
        None,
        Some(PathBuf::from(".")),
        Some(PathBuf::from("relative-user-profile")),
    ])
    .expect_err("relative home candidates must never become path authority");

    assert_eq!(error, "home-directory-unavailable");
}

#[test]
fn home_resolution_preserves_first_absolute_candidate_precedence() {
    let app_home = absolute_fixture("app-home");
    let env_home = absolute_fixture("env-home");
    let resolved = home_resolution::select_absolute_home([
        Some(app_home.clone()),
        Some(env_home),
    ])
    .expect("the first absolute home candidate should win");

    assert_eq!(resolved, app_home);
}
