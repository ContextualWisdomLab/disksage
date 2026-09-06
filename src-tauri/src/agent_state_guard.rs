//! Agent conversations and their supporting state are never disposable caches.
use std::path::{Component, Path, PathBuf};

/// Match complete path components, including project-local agent state.
fn has_state_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(component, Component::Normal(name) if [".codex", ".claude", ".claude.json"]
            .iter().any(|marker| name.eq_ignore_ascii_case(marker)))
    })
}

/// Include both ancestors and descendants: deleting a parent also deletes its sessions.
fn overlaps(path: &Path, root: &Path) -> bool {
    #[cfg(windows)]
    let (path, root) = (
        PathBuf::from(path.as_os_str().to_ascii_lowercase()),
        PathBuf::from(root.as_os_str().to_ascii_lowercase()),
    );
    path.starts_with(&root) || root.starts_with(&path)
}

/// Resolve the existing ancestor too when a destination does not exist yet.
fn resolve_existing_parent(path: &Path) -> PathBuf {
    let mut probe = path;
    let mut suffix = Vec::new();
    loop {
        if let Ok(mut resolved) = std::fs::canonicalize(probe) {
            for name in suffix.iter().rev() {
                resolved.push(name);
            }
            return resolved;
        }
        match (probe.file_name(), probe.parent()) {
            (Some(name), Some(parent)) => {
                suffix.push(name);
                probe = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Protect lexical and resolved roots, so a symlink cannot disguise relocated state.
fn protects_prepared(path: &Path, roots: &[(PathBuf, PathBuf)]) -> bool {
    let Ok(path) = std::path::absolute(path) else {
        return true;
    };
    if has_state_component(&path) {
        return true;
    }
    let resolved = resolve_existing_parent(&path);
    has_state_component(&resolved)
        || roots.iter().any(|(root, canonical)| {
            overlaps(&path, root)
                || overlaps(&resolved, root)
                || overlaps(&path, canonical)
                || overlaps(&resolved, canonical)
        })
}

fn prepare_roots(roots: Vec<PathBuf>) -> Vec<(PathBuf, PathBuf)> {
    roots
        .into_iter()
        .map(|root| {
            let canonical = resolve_existing_parent(&root);
            (root, canonical)
        })
        .collect()
}

#[cfg(test)]
fn protects_with_roots(path: &Path, roots: &[PathBuf]) -> bool {
    protects_prepared(path, &prepare_roots(roots.to_vec()))
}

/// An unusable configured root is inconclusive and blocks cleanup.
fn configured_roots() -> Option<Vec<(PathBuf, PathBuf)>> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        let home = PathBuf::from(home);
        if !home.is_absolute() {
            return None;
        }
        roots.extend([
            home.join(".codex"),
            home.join(".claude"),
            home.join(".claude.json"),
        ]);
    }
    for key in ["CODEX_HOME", "CLAUDE_CONFIG_DIR"] {
        if let Some(root) = std::env::var_os(key) {
            let root = PathBuf::from(root);
            if !root.is_absolute() {
                return None;
            }
            roots.push(root);
        }
    }
    Some(prepare_roots(roots))
}

/// Preserve default and explicitly relocated agent state without opening its contents.
pub fn is_agent_state(path: &Path) -> bool {
    configured_roots().is_none_or(|roots| protects_prepared(path, &roots))
}

/// Refuse a directory containing nested agent state, or an incomplete metadata walk.
/// Symlink entries are inspected but never traversed. No conversation contents are read.
pub fn contains_agent_state(path: &Path) -> bool {
    contains_with_limit(path, 10_000)
}

fn contains_with_limit(path: &Path, limit: usize) -> bool {
    let Some(roots) = configured_roots() else {
        return true;
    };
    // Scope the snapshot to one walk, and retain if its configuration or targets changed.
    contains_with_roots(path, limit, &roots) || configured_roots().as_ref() != Some(&roots)
}

fn contains_with_roots(path: &Path, limit: usize, roots: &[(PathBuf, PathBuf)]) -> bool {
    if protects_prepared(path, roots) {
        return true;
    }
    let mut pending = vec![path.to_path_buf()];
    let mut visited = 0;
    while let Some(path) = pending.pop() {
        visited += 1;
        if visited > limit || protects_prepared(&path, roots) {
            return true;
        }
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            // A missing initial target cannot be deleted; keep the caller's normal error path.
            Err(error) if visited == 1 && error.kind() == std::io::ErrorKind::NotFound => {
                return false
            }
            Err(_) => return true,
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&path) else {
            return true;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                return true;
            };
            // ponytail: bounded metadata walk; use owner-issued manifests for larger trees.
            if visited + pending.len() >= limit {
                return true;
            }
            pending.push(entry.path());
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_preservation_metric() {
        let base = std::env::current_dir().unwrap();
        let protected = [
            ".codex/sessions/2026/09/06/rollout.jsonl",
            ".codex/archived_sessions/session.jsonl",
            ".codex/state_5.sqlite",
            ".codex/session_index.jsonl",
            ".claude/projects/project/session.jsonl",
            ".claude/file-history/session/file",
            ".claude/history.jsonl",
            ".claude/projects/project/session/tool-results/output.txt",
        ];
        let false_positives = protected
            .iter()
            .filter(|path| !is_agent_state(&base.join(path)))
            .count();
        println!(
            "protected_cases={} false_positive_candidates={} false_positive_rate={:.6}",
            protected.len(),
            false_positives,
            false_positives as f64 / protected.len() as f64
        );
        assert_eq!(false_positives, 0);
        for allowed in [
            "node_modules/package/index.js",
            "target/debug/build.o",
            ".codex-backup-not-state",
            ".claude-cache-not-state",
        ] {
            assert!(!is_agent_state(&base.join(allowed)), "{allowed}");
        }
    }

    #[test]
    fn configured_roots_protect_ancestors_and_not_siblings() {
        let base = std::env::current_dir().unwrap().join("guard-fixture");
        let root = base.join("agent-data");
        for path in [&base, &root, &root.join("sessions/session.jsonl")] {
            assert!(protects_with_roots(path, &[root.clone()]));
        }
        assert!(!protects_with_roots(
            &base.join("agent-data-cache"),
            &[root]
        ));
        assert!(has_state_component(&base.join(".CoDeX/sessions/record")));
    }

    #[cfg(unix)]
    #[test]
    fn nested_state_aliases_and_incomplete_walks_are_retained() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("disksage-agent-guard-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let project = root.join("project");
        std::fs::create_dir(&project).unwrap();
        std::fs::write(project.join("build.o"), b"generated").unwrap();
        assert!(!contains_agent_state(&project));
        assert!(contains_with_limit(&project, 1));
        let state = project.join("nested/.claude/projects");
        std::fs::create_dir_all(&state).unwrap();
        assert!(contains_agent_state(&project));
        let alias = root.join("alias");
        symlink(&state, &alias).unwrap();
        assert!(is_agent_state(&alias));
        let relocated = root.join("relocated");
        std::fs::create_dir(&relocated).unwrap();
        let configured = root.join(".codex");
        symlink(&relocated, &configured).unwrap();
        assert!(protects_with_roots(
            &relocated.join("session.jsonl"),
            &[configured.clone()]
        ));
        let roots = vec![configured.clone()];
        let prepared = prepare_roots(roots.clone());
        for path in [
            root.clone(),
            relocated.clone(),
            relocated.join("missing/session.jsonl"),
            alias.clone(),
            project.join("build.o"),
            root.join("unrelated/missing"),
        ] {
            // The previous per-entry algorithm remains the equivalence oracle.
            let resolved = resolve_existing_parent(&path);
            let previous = has_state_component(&path)
                || has_state_component(&resolved)
                || roots.iter().any(|root| {
                    let canonical = resolve_existing_parent(root);
                    overlaps(&path, root)
                        || overlaps(&resolved, root)
                        || overlaps(&path, &canonical)
                        || overlaps(&resolved, &canonical)
                });
            assert_eq!(
                protects_prepared(&path, &prepared),
                previous,
                "{}",
                path.display()
            );
        }
        let replacement = root.join("replacement");
        std::fs::create_dir(&replacement).unwrap();
        std::fs::remove_file(&configured).unwrap();
        symlink(&replacement, &configured).unwrap();
        assert_ne!(prepared, prepare_roots(roots));
        // Only this test's create-new fixture is removed; user session roots are never touched.
        std::fs::remove_dir_all(root).unwrap();
    }
}
