//! Bounded, read-only allocated-block inventory for customer-selected roots.

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AllocationMapEntry {
    pub root: String,
    pub allocated_bytes: u64,
    pub visited_entries: u64,
    pub classification: &'static str,
    pub evidence_complete: bool,
    pub stop_reason: Option<&'static str>,
}

fn normal_components(path: &Path) -> Vec<&std::ffi::OsStr> {
    let mut normalized = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => normalized.push(value),
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir => normalized.clear(),
            Component::CurDir | Component::Prefix(_) => {}
        }
    }
    normalized
}

fn contains_component_sequence(components: &[&std::ffi::OsStr], sequence: &[&str]) -> bool {
    !sequence.is_empty()
        && components.windows(sequence.len()).any(|window| {
            window
                .iter()
                .zip(sequence)
                .all(|(component, expected)| *component == std::ffi::OsStr::new(expected))
        })
}

fn contains_component(components: &[&std::ffi::OsStr], expected: &str) -> bool {
    components
        .iter()
        .any(|component| *component == std::ffi::OsStr::new(expected))
}

fn classification(path: &Path) -> &'static str {
    let components = normal_components(path);
    if contains_component_sequence(&components, &["Library", "Mobile Documents"])
        || contains_component_sequence(&components, &["Library", "CloudStorage"])
        || contains_component_sequence(
            &components,
            &["Library", "Application Support", "OneDrive"],
        )
        || contains_component_sequence(
            &components,
            &["Library", "Application Support", "FileProvider"],
        )
        || contains_component_sequence(
            &components,
            &["Library", "Application Support", "CloudDocs"],
        )
    {
        "provider-managed"
    } else if components.iter().any(|component| {
        let value = component.to_string_lossy();
        value.ends_with(".photoslibrary") || value.ends_with(".photolibrary")
    }) {
        "photos-managed"
    } else if contains_component(&components, "Parallels")
        || contains_component(&components, ".colima")
        || contains_component_sequence(&components, &["containers", "podman"])
        || contains_component_sequence(&components, &["private", "var", "vm"])
    {
        "virtual-machine-managed"
    } else if contains_component_sequence(&components, &["private", "var", "folders"])
    {
        "user-cache-and-temporary"
    } else if contains_component(&components, "Caches") || contains_component(&components, ".cache")
    {
        "cache"
    } else if path.file_name().is_some_and(|name| {
        name == std::ffi::OsStr::new("target") || name == std::ffi::OsStr::new("node_modules")
    }) || contains_component_sequence(&components, &[".cargo", "registry"])
    {
        "generated"
    } else {
        "user-or-application-data"
    }
}

fn collect_children_within_budget<I>(
    mut entries: I,
    remaining_entries: u64,
    started: Instant,
    max_duration: Duration,
) -> Result<Vec<PathBuf>, &'static str>
where
    I: Iterator<Item = std::io::Result<PathBuf>>,
{
    let mut children = Vec::new();
    loop {
        if started.elapsed() >= max_duration {
            return Err("duration-limit-reached");
        }
        match entries.next() {
            None => {
                children.sort();
                return Ok(children);
            }
            Some(Err(_)) => return Err("directory-entry-unreadable"),
            Some(Ok(path)) => {
                if children.len() as u64 >= remaining_entries {
                    return Err("entry-limit-reached");
                }
                children.push(path);
            }
        }
    }
}

/// Sum allocated filesystem blocks without following symlinks or crossing a device boundary.
pub fn measure_root(
    root: &Path,
    max_entries: u64,
    max_duration: Duration,
) -> Result<AllocationMapEntry, String> {
    use std::os::unix::fs::MetadataExt;
    if !root.is_absolute() || max_entries == 0 || max_duration.is_zero() {
        return Err("allocation-map-options-invalid".into());
    }
    let root_metadata = std::fs::symlink_metadata(root)
        .map_err(|_| "allocation-map-root-unavailable".to_string())?;
    if root_metadata.file_type().is_symlink() {
        return Err("allocation-map-root-symlink-rejected".into());
    }
    // Classification must describe the location the kernel actually resolves. A textual
    // `component/..` pair is not equivalent when `component` is an intermediate symlink.
    let resolved_root = std::fs::canonicalize(root)
        .map_err(|_| "allocation-map-root-unavailable".to_string())?;
    let device = root_metadata.dev();
    let started = Instant::now();
    let mut stack = vec![root.to_path_buf()];
    let mut allocated_identities = HashSet::new();
    let mut allocated_bytes = 0_u64;
    let mut visited_entries = 0_u64;
    let mut stop_reason = None;
    while let Some(path) = stack.pop() {
        if visited_entries >= max_entries {
            stop_reason = Some("entry-limit-reached");
            break;
        }
        if started.elapsed() >= max_duration {
            stop_reason = Some("duration-limit-reached");
            break;
        }
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(_) => {
                stop_reason = Some("metadata-unavailable");
                break;
            }
        };
        if metadata.dev() != device {
            continue;
        }
        visited_entries += 1;
        if allocated_identities.insert((metadata.dev(), metadata.ino())) {
            allocated_bytes = allocated_bytes.saturating_add(metadata.blocks().saturating_mul(512));
        }
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            let queued_entries = u64::try_from(stack.len()).unwrap_or(u64::MAX);
            let remaining_entries = max_entries
                .saturating_sub(visited_entries)
                .saturating_sub(queued_entries);
            let entries = match std::fs::read_dir(&path) {
                Ok(entries) => entries,
                Err(_) => {
                    stop_reason = Some("directory-unreadable");
                    break;
                }
            };
            let children = match collect_children_within_budget(
                entries.map(|entry| entry.map(|value| value.path())),
                remaining_entries,
                started,
                max_duration,
            ) {
                Ok(children) => children,
                Err(reason) => {
                    stop_reason = Some(reason);
                    break;
                }
            };
            stack.extend(children.into_iter().rev());
        }
    }
    Ok(AllocationMapEntry {
        root: root.to_string_lossy().into_owned(),
        allocated_bytes,
        visited_entries,
        classification: classification(&resolved_root),
        evidence_complete: stop_reason.is_none(),
        stop_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn bounds_entries_and_never_follows_symlinks() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(&outside, vec![0_u8; 4096]).unwrap();
        symlink(&outside, root.join("link")).unwrap();
        let report = measure_root(&root, 2, Duration::from_secs(1)).unwrap();
        assert_eq!(report.visited_entries, 2);
        assert!(report.evidence_complete);
        assert!(report.allocated_bytes < 4096);
        assert_eq!(
            measure_root(&root, 1, Duration::from_secs(1))
                .unwrap()
                .stop_reason,
            Some("entry-limit-reached")
        );
    }

    #[test]
    fn classifies_provider_vm_and_per_user_var_boundaries() {
        for path in [
            "/Users/test/Library/Mobile Documents",
            "/Users/test/Library/CloudStorage",
        ] {
            assert_eq!(classification(Path::new(path)), "provider-managed", "{path}");
        }
        for path in [
            "/Users/test/Parallels",
            "/Users/test/.colima",
            "/Users/test/.local/share/containers/podman",
        ] {
            assert_eq!(
                classification(Path::new(path)),
                "virtual-machine-managed",
                "{path}"
            );
        }
        assert_eq!(
            classification(Path::new("/private/var/folders")),
            "user-cache-and-temporary"
        );
        assert_eq!(
            classification(Path::new("/Users/test/album.photoslibrary-notes")),
            "user-or-application-data"
        );
        for ordinary in [
            "/tmp/Library/../Mobile Documents",
            "/tmp/.cargo/../registry",
            "/tmp/private/var/../folders",
            "/tmp/containers/../podman",
        ] {
            assert_eq!(classification(Path::new(ordinary)), "user-or-application-data");
        }
    }

    #[test]
    fn classifies_the_resolved_root_after_intermediate_symlink_parent_traversal() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("Library");
        let actual_parent = temp.path().join("ordinary");
        let symlink_target = actual_parent.join("nested");
        let actual_root = actual_parent.join("Mobile Documents");
        std::fs::create_dir(&library).unwrap();
        std::fs::create_dir_all(&symlink_target).unwrap();
        std::fs::create_dir(&actual_root).unwrap();
        symlink(&symlink_target, library.join("link")).unwrap();

        let selected_root = library.join("link/../Mobile Documents");
        assert_eq!(classification(&selected_root), "provider-managed");
        let report = measure_root(&selected_root, 1, Duration::from_secs(1)).unwrap();
        assert_ne!(report.classification, "provider-managed");
    }

    #[test]
    fn hard_links_do_not_double_count_allocated_blocks() {
        use std::os::unix::fs::MetadataExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let first = root.join("first.bin");
        let second = root.join("second.bin");
        std::fs::write(&first, vec![0_u8; 4096]).unwrap();
        std::fs::hard_link(&first, &second).unwrap();

        let root_bytes = std::fs::symlink_metadata(&root).unwrap().blocks() * 512;
        let file_bytes = std::fs::symlink_metadata(&first).unwrap().blocks() * 512;
        assert!(file_bytes > 0);
        let report = measure_root(&root, 3, Duration::from_secs(1)).unwrap();

        assert_eq!(report.visited_entries, 3);
        assert_eq!(report.allocated_bytes, root_bytes + file_bytes);
        assert!(report.evidence_complete);
    }

    struct CountingPaths {
        calls: Rc<Cell<usize>>,
        remaining: usize,
    }

    impl Iterator for CountingPaths {
        type Item = std::io::Result<PathBuf>;

        fn next(&mut self) -> Option<Self::Item> {
            self.calls.set(self.calls.get() + 1);
            if self.remaining == 0 {
                return None;
            }
            self.remaining -= 1;
            Some(Ok(PathBuf::from(format!("child-{}", self.remaining))))
        }
    }

    #[test]
    fn directory_enumeration_stops_after_remaining_budget_plus_overflow_probe() {
        let calls = Rc::new(Cell::new(0));
        let iterator = CountingPaths {
            calls: calls.clone(),
            remaining: 10_000,
        };
        let started = Instant::now();

        let error = collect_children_within_budget(
            iterator,
            3,
            started,
            Duration::from_secs(1),
        )
        .unwrap_err();

        assert_eq!(error, "entry-limit-reached");
        assert!(calls.get() <= 4, "enumerated {} entries", calls.get());
    }

    #[test]
    fn directory_iterator_error_marks_evidence_incomplete() {
        let iterator = vec![
            Ok(PathBuf::from("readable")),
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "fixture entry denied",
            )),
        ]
        .into_iter();

        let error = collect_children_within_budget(
            iterator,
            8,
            Instant::now(),
            Duration::from_secs(1),
        )
        .unwrap_err();

        assert_eq!(error, "directory-entry-unreadable");
    }
}
