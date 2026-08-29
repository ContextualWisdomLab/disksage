use std::fs;
use std::path::PathBuf;

#[test]
fn finder_copy_cancel_applescript_has_an_explicit_statement_separator() {
    let source_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("provider_recovery.rs");
    let source = fs::read_to_string(source_path).expect("provider recovery source must be readable");

    assert!(
        source.contains(
            "tell application \\\"Finder\\\" to activate\\ntell application \\\"System Events\\\" to tell process \\\"Finder\\\" to key code 53\\n",
        ),
        "Finder cancel AppleScript must retain a real statement separator instead of Rust line continuation",
    );
}
