use std::path::PathBuf;

fn brew_cleanup_source() -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/brew_cleanup.rs"))
        .expect("brew cleanup production source must be readable")
}

#[test]
fn standard_intel_homebrew_repository_target_is_admitted_without_following_alias_symlinks() {
    let source = brew_cleanup_source();
    let (_, macos_implementation) = source
        .split_once("#[cfg(target_os = \"macos\")]\nfn fixed_brew_path()")
        .expect("macOS fixed Homebrew executable admission must remain present");
    let (fixed_brew_path, _) = macos_implementation
        .split_once("#[cfg(not(target_os = \"macos\"))]")
        .expect("non-macOS fail-closed boundary must remain present");

    assert!(
        fixed_brew_path.contains("Path::new(\"/usr/local/Homebrew/bin/brew\")"),
        "fixed candidates must include the standard Intel Homebrew repository target"
    );
    assert!(
        fixed_brew_path.contains("!metadata.file_type().is_symlink()"),
        "Intel compatibility must not weaken symbolic-link rejection"
    );
}
