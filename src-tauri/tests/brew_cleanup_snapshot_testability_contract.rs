use std::fs;
use std::path::PathBuf;

fn brew_cleanup_source() -> String {
    fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/brew_cleanup.rs"))
        .expect("brew cleanup production source must be readable")
}

#[test]
fn verified_brew_snapshot_boundary_is_exercisable_in_unix_tests_only() {
    let source = brew_cleanup_source();
    let testable_unix_cfg = "#[cfg(any(target_os = \"macos\", all(test, unix)))]";

    assert!(
        source.contains(&format!(
            "{testable_unix_cfg}\nstruct VerifiedBrewExecutable"
        )),
        "the verified executable holder must remain macOS production code while becoming exercisable in Unix unit tests"
    );
    assert!(
        source.contains(&format!("{testable_unix_cfg}\nfn open_verified_brew")),
        "the exact executable opener must be testable on the Linux CI runner without broadening runtime platform support"
    );
}
