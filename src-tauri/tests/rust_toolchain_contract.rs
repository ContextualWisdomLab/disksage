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

fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn yaml_scalar_in_block(
    lines: &[&str],
    block_start: usize,
    block_indent: usize,
    target_key: &str,
) -> Option<String> {
    for line in &lines[block_start + 1..] {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent <= block_indent {
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            if key.trim() == target_key && !value.trim().is_empty() {
                return Some(unquote(value).to_string());
            }
        }
    }
    None
}

fn yaml_nested_scalar_in_block(
    lines: &[&str],
    block_start: usize,
    block_indent: usize,
    parent_key: &str,
    target_key: &str,
) -> Option<String> {
    for (offset, line) in lines[block_start + 1..].iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent <= block_indent {
            break;
        }
        if trimmed == format!("{parent_key}:") {
            return yaml_scalar_in_block(lines, block_start + 1 + offset, indent, target_key);
        }
    }
    None
}

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

fn entry_scalar(entry: &[&str], target_key: &str) -> Option<String> {
    for line in entry {
        let trimmed = line.trim();
        let Some((key, value)) = trimmed.strip_prefix("- ").unwrap_or(trimmed).split_once(':') else {
            continue;
        };
        if key.trim() == target_key && !value.trim().is_empty() {
            return Some(unquote(value).to_string());
        }
    }
    None
}

fn entry_nested_scalar(entry: &[&str], parent_key: &str, target_key: &str) -> Option<String> {
    for (index, line) in entry.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed != format!("{parent_key}:") {
            continue;
        }
        return yaml_scalar_in_block(entry, index, leading_spaces(line), target_key);
    }
    None
}

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

#[test]
fn compiler_updates_are_reviewable() {
    let rust_update = dependabot_entry(DEPENDABOT, "rust-toolchain")
        .expect("Dependabot must contain a rust-toolchain update entry");
    assert_eq!(entry_scalar(&rust_update, "directory").as_deref(), Some("/"));
    assert_eq!(
        entry_nested_scalar(&rust_update, "schedule", "interval").as_deref(),
        Some("weekly")
    );
    assert_eq!(
        entry_scalar(&rust_update, "open-pull-requests-limit").as_deref(),
        Some("1")
    );
}

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
