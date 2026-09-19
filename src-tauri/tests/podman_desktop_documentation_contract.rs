//! Source-level documentation contract for the Podman desktop evidence module.
//!
//! This test keeps private helpers and regression tests understandable in addition to the public
//! API rustdoc enforced by the module's `missing_docs` lint.

use std::fs;
use std::path::PathBuf;

/// Require every named function in the Podman desktop evidence module to have adjacent,
/// beginner-readable rustdoc rather than an empty marker or placeholder text.
#[test]
fn every_podman_desktop_function_has_beginner_readable_rustdoc() {
    let source_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/podman_desktop.rs");
    let source = fs::read_to_string(&source_path).expect("podman_desktop.rs must be readable");
    let lines = source.lines().collect::<Vec<_>>();
    let mut violations = Vec::new();

    for (line_index, line) in lines.iter().enumerate() {
        let declaration = line.trim_start();
        let is_named_function = declaration.starts_with("fn ")
            || declaration.starts_with("pub fn ")
            || declaration.starts_with("pub(crate) fn ")
            || declaration.starts_with("async fn ")
            || declaration.starts_with("pub async fn ")
            || declaration.starts_with("pub(crate) async fn ")
            || declaration.starts_with("unsafe fn ")
            || declaration.starts_with("pub unsafe fn ")
            || declaration.starts_with("pub(crate) unsafe fn ")
            || declaration.starts_with("const fn ")
            || declaration.starts_with("pub const fn ")
            || declaration.starts_with("pub(crate) const fn ");
        if !is_named_function {
            continue;
        }

        let mut cursor = line_index;
        while cursor > 0 {
            let previous = lines[cursor - 1].trim();
            if previous.is_empty() || previous.starts_with("#[") {
                cursor -= 1;
                continue;
            }
            break;
        }

        let mut rustdoc_lines = Vec::new();
        while cursor > 0 {
            let previous = lines[cursor - 1].trim();
            let Some(rustdoc) = previous.strip_prefix("///") else {
                break;
            };
            rustdoc_lines.push(rustdoc.trim());
            cursor -= 1;
        }
        rustdoc_lines.reverse();
        let rustdoc = rustdoc_lines.join(" ");
        let readable = rustdoc.chars().count() >= 24
            && !rustdoc.to_ascii_lowercase().contains("todo")
            && !rustdoc.to_ascii_lowercase().contains("placeholder");
        if !readable {
            violations.push(format!("line {}: {declaration}", line_index + 1));
        }
    }

    assert!(
        violations.is_empty(),
        "every Podman desktop function needs adjacent beginner-readable rustdoc; violations: {}",
        violations.join(", ")
    );
}
