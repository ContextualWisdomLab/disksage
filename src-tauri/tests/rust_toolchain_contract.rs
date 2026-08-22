//! Repository contracts for DiskSage's reviewed Rust compiler baseline.
//!
//! The configuration readers below intentionally validate hierarchy instead of searching for
//! matching text anywhere in a file. This keeps comments, unrelated TOML tables, and sibling
//! YAML blocks from becoming false evidence for the compiler-governance contract.

const EXPECTED_RUST_VERSION: &str = "1.97.1";
const RUST_TOOLCHAIN: &str = include_str!("../../rust-toolchain.toml");
const CARGO_MANIFEST: &str = include_str!("../Cargo.toml");
const TEST_WORKFLOW: &str = include_str!("../../.github/workflows/test.yml");
const RELEASE_WORKFLOW: &str = include_str!("../../.github/workflows/release.yml");
const DEPENDABOT: &str = include_str!("../../.github/dependabot.yml");
const RUST_TOOLCHAIN_ACTION: &str = "dtolnay/rust-toolchain@";

/// Remove one layer of TOML/YAML quoting from a scalar value.
fn unquote(value: &str) -> &str {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

/// Read one scalar from the requested TOML table only.
fn toml_scalar(source: &str, target_table: &str, target_key: &str) -> Option<String> {
    let mut current_table = None;
    for raw_line in source.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_table = Some(line[1..line.len() - 1].trim());
            continue;
        }
        if current_table != Some(target_table) {
            continue;
        }
        let (key, value) = line.split_once('=')?;
        if key.trim() == target_key {
            return Some(unquote(value).to_string());
        }
    }
    None
}

/// Count leading ASCII spaces used by the repository's two-space YAML style.
fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// Read a scalar whose key is an immediate child of a YAML mapping block.
fn yaml_scalar_in_block(
    lines: &[&str],
    block_start: usize,
    block_indent: usize,
    target_key: &str,
) -> Option<String> {
    let expected_indent = block_indent + 2;
    for line in &lines[block_start + 1..] {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent <= block_indent {
            break;
        }
        if indent != expected_indent {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            if key.trim() == target_key && !value.trim().is_empty() {
                return Some(unquote(value).to_string());
            }
        }
    }
    None
}

/// Read a scalar from a named immediate child mapping, never from deeper nesting.
fn yaml_nested_scalar_in_block(
    lines: &[&str],
    block_start: usize,
    block_indent: usize,
    parent_key: &str,
    target_key: &str,
) -> Option<String> {
    let expected_indent = block_indent + 2;
    for (offset, line) in lines[block_start + 1..].iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent <= block_indent {
            break;
        }
        if indent != expected_indent {
            continue;
        }
        if trimmed == format!("{parent_key}:") {
            return yaml_scalar_in_block(lines, block_start + 1 + offset, indent, target_key);
        }
    }
    None
}

/// Collect `toolchain` values from each reviewed Rust toolchain action block.
fn action_toolchains(source: &str) -> Vec<Option<String>> {
    let lines: Vec<_> = source.lines().collect();
    let mut values = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let Some(uses) = trimmed.strip_prefix("- uses:") else {
            continue;
        };
        if !unquote(uses).starts_with(RUST_TOOLCHAIN_ACTION) {
            continue;
        }
        values.push(yaml_nested_scalar_in_block(
            &lines,
            index,
            leading_spaces(line),
            "with",
            "toolchain",
        ));
    }
    values
}

/// Return one Dependabot update item for the requested ecosystem.
fn dependabot_entry<'a>(source: &'a str, ecosystem: &str) -> Option<Vec<&'a str>> {
    let lines: Vec<_> = source.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let Some(value) = trimmed.strip_prefix("- package-ecosystem:") else {
            continue;
        };
        if unquote(value) != ecosystem {
            continue;
        }
        let item_indent = leading_spaces(line);
        let mut entry = vec![*line];
        for next in &lines[index + 1..] {
            let next_trimmed = next.trim();
            if next_trimmed.is_empty() || next_trimmed.starts_with('#') {
                continue;
            }
            let indent = leading_spaces(next);
            if indent <= item_indent {
                break;
            }
            entry.push(*next);
        }
        return Some(entry);
    }
    None
}

/// Read a direct scalar field from one bounded Dependabot item.
fn entry_scalar(entry: &[&str], target_key: &str) -> Option<String> {
    let item_indent = entry.first().map(|line| leading_spaces(line))?;
    let expected_indent = item_indent + 2;
    for line in &entry[1..] {
        let indent = leading_spaces(line);
        if indent <= item_indent {
            break;
        }
        if indent != expected_indent {
            continue;
        }
        let trimmed = line.trim();
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        if key.trim() == target_key && !value.trim().is_empty() {
            return Some(unquote(value).to_string());
        }
    }
    None
}

/// Read a scalar from a direct nested mapping in one bounded Dependabot item.
fn entry_nested_scalar(entry: &[&str], parent_key: &str, target_key: &str) -> Option<String> {
    let item_indent = entry.first().map(|line| leading_spaces(line))?;
    let expected_indent = item_indent + 2;
    for (index, line) in entry.iter().enumerate().skip(1) {
        let indent = leading_spaces(line);
        if indent <= item_indent {
            break;
        }
        if indent != expected_indent {
            continue;
        }
        let trimmed = line.trim();
        if trimmed != format!("{parent_key}:") {
            continue;
        }
        return yaml_scalar_in_block(entry, index, expected_indent, target_key);
    }
    None
}

/// Verify the local manifest and every CI Rust action use the exact compiler baseline.
#[test]
fn local_package_and_ci_use_the_same_exact_compiler() {
    assert_eq!(
        toml_scalar(RUST_TOOLCHAIN, "toolchain", "channel").as_deref(),
        Some(EXPECTED_RUST_VERSION)
    );
    assert_eq!(
        toml_scalar(CARGO_MANIFEST, "package", "rust-version").as_deref(),
        Some(EXPECTED_RUST_VERSION)
    );

    let ci_toolchains = action_toolchains(TEST_WORKFLOW);
    assert_eq!(ci_toolchains.len(), 2, "both Rust CI jobs must install a reviewed toolchain");
    assert!(
        ci_toolchains
            .iter()
            .all(|version| version.as_deref() == Some(EXPECTED_RUST_VERSION)),
        "every Rust CI action must bind with.toolchain to the exact baseline"
    );
}

/// Verify release jobs inherit the repository toolchain instead of overriding it.
#[test]
fn release_commands_remain_under_the_root_toolchain_override() {
    let release_toolchains = action_toolchains(RELEASE_WORKFLOW);
    assert_eq!(
        release_toolchains.len(),
        2,
        "release and GPU release jobs must retain their Rust bootstrap action"
    );
    assert!(
        release_toolchains.iter().all(Option::is_none),
        "release Rust actions must not override the repository rust-toolchain.toml authority"
    );
    assert!(RELEASE_WORKFLOW.contains(
        "cargo build --manifest-path src-tauri/Cargo.toml --release --features cloud-cli"
    ));
    assert!(RELEASE_WORKFLOW.contains("npm run tauri -- build --features llm-engine"));
    assert!(!RELEASE_WORKFLOW.contains("working-directory: src-tauri"));
}

/// Verify Dependabot's real ecosystems stay reviewable and the schema stays valid.
///
/// `rust-toolchain` is not a package-ecosystem value GitHub Dependabot recognizes (its
/// supported ecosystems are enumerated at
/// <https://docs.github.com/en/code-security/dependabot/ecosystems-supported-by-dependabot/supported-ecosystems-and-repositories>,
/// and `rust-toolchain.toml` tracking is not among them). An unrecognized `package-ecosystem`
/// value fails Dependabot's config schema validation for the *entire file*, silently disabling
/// the existing `github-actions`, `npm`, and `cargo` update jobs too. This contract guards
/// against reintroducing that entry and instead verifies the ecosystems Dependabot actually
/// supports remain configured with a bounded weekly review cadence.
#[test]
fn compiler_updates_are_reviewable() {
    assert!(
        dependabot_entry(DEPENDABOT, "rust-toolchain").is_none(),
        "rust-toolchain is not a supported Dependabot package-ecosystem; an entry here \
         invalidates the whole config file and silently disables every other ecosystem's updates"
    );

    let cargo_update = dependabot_entry(DEPENDABOT, "cargo")
        .expect("Dependabot must track the src-tauri Cargo manifest, which pulls in rust-toolchain.toml-adjacent crate updates");
    assert_eq!(entry_scalar(&cargo_update, "directory").as_deref(), Some("/src-tauri"));
    assert_eq!(
        entry_nested_scalar(&cargo_update, "schedule", "interval").as_deref(),
        Some("weekly")
    );

    let actions_update = dependabot_entry(DEPENDABOT, "github-actions")
        .expect("Dependabot must track workflow action pins");
    assert_eq!(
        entry_nested_scalar(&actions_update, "schedule", "interval").as_deref(),
        Some("weekly")
    );
}

/// Prove comments, sibling mappings, and deeper mappings cannot satisfy the contract.
#[test]
fn structural_readers_reject_decoys_and_wrong_hierarchy() {
    let toml_decoy = r#"
# channel = "1.97.1"
[other]
channel = "1.97.1"
[toolchain]
channel = "stable"
"#;
    assert_eq!(
        toml_scalar(toml_decoy, "toolchain", "channel").as_deref(),
        Some("stable")
    );

    let yaml_decoy = r#"
steps:
  - uses: dtolnay/rust-toolchain@deadbeef
    env:
      toolchain: 1.97.1
    with:
      components: rustfmt
  - name: unrelated
    with:
      toolchain: 1.97.1
"#;
    assert_eq!(action_toolchains(yaml_decoy), vec![None]);

    let nested_yaml_decoy = r#"
steps:
  - uses: dtolnay/rust-toolchain@deadbeef
    with:
      nested:
        toolchain: 1.97.1
"#;
    assert_eq!(action_toolchains(nested_yaml_decoy), vec![None]);

    let dependabot_decoy = r#"
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
    note: "rust-toolchain"
  - package-ecosystem: "rust-toolchain"
    directory: "/wrong"
    schedule:
      interval: "daily"
"#;
    let entry = dependabot_entry(dependabot_decoy, "rust-toolchain").unwrap();
    assert_eq!(entry_scalar(&entry, "directory").as_deref(), Some("/wrong"));
    assert_eq!(
        entry_nested_scalar(&entry, "schedule", "interval").as_deref(),
        Some("daily")
    );

    let nested_dependabot_decoy = r#"
updates:
  - package-ecosystem: "rust-toolchain"
    metadata:
      directory: "/"
    schedule:
      nested:
        interval: "weekly"
    open-pull-requests-limit: 1
"#;
    let nested_entry = dependabot_entry(nested_dependabot_decoy, "rust-toolchain").unwrap();
    assert_eq!(entry_scalar(&nested_entry, "directory"), None);
    assert_eq!(entry_nested_scalar(&nested_entry, "schedule", "interval"), None);
}
