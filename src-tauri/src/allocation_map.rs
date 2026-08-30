//! Bounded, read-only allocated-block inventory for customer-selected roots.

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Component, Path};
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

fn collect_children_within_budget<I, T, E>(
    mut entries: I,
    remaining_entries: u64,
    started: Instant,
    max_duration: Duration,
) -> Result<Vec<T>, &'static str>
where
    I: Iterator<Item = Result<T, E>>,
    T: Ord,
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
            Some(Ok(value)) => {
                if children.len() as u64 >= remaining_entries {
                    return Err("entry-limit-reached");
                }
                children.push(value);
            }
        }
    }
}

#[cfg(unix)]
mod unix_bound {
    use super::*;
    use std::ffi::{CStr, CString, OsStr, OsString};
    use std::mem::MaybeUninit;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    #[cfg(target_os = "linux")]
    unsafe fn errno_location() -> *mut libc::c_int {
        libc::__errno_location()
    }

    #[cfg(target_os = "macos")]
    unsafe fn errno_location() -> *mut libc::c_int {
        libc::__error()
    }

    fn path_cstring(path: &Path) -> Result<CString, &'static str> {
        CString::new(path.as_os_str().as_bytes()).map_err(|_| "allocation-map-path-invalid")
    }

    fn name_cstring(name: &OsStr) -> Result<CString, &'static str> {
        CString::new(name.as_bytes()).map_err(|_| "allocation-map-path-invalid")
    }

    fn open_bound_root(path: &Path) -> Result<OwnedFd, &'static str> {
        let path = path_cstring(path)?;
        let fd = unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            )
        };
        if fd < 0 {
            return Err("allocation-map-root-unavailable");
        }
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    fn stat_fd(fd: &OwnedFd) -> Result<libc::stat, &'static str> {
        let mut stat = MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(fd.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return Err("metadata-unavailable");
        }
        Ok(unsafe { stat.assume_init() })
    }

    fn stat_child(parent: &OwnedFd, name: &OsStr) -> Result<libc::stat, &'static str> {
        let name = name_cstring(name)?;
        let mut stat = MaybeUninit::<libc::stat>::uninit();
        if unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err("metadata-unavailable");
        }
        Ok(unsafe { stat.assume_init() })
    }

    fn stat_identity(stat: &libc::stat) -> (u64, u64) {
        (stat.st_dev as u64, stat.st_ino as u64)
    }

    fn metadata_identity(metadata: &std::fs::Metadata) -> (u64, u64) {
        use std::os::unix::fs::MetadataExt;
        (metadata.dev(), metadata.ino())
    }

    fn is_directory(stat: &libc::stat) -> bool {
        (stat.st_mode & libc::S_IFMT) == libc::S_IFDIR
    }

    fn allocated_bytes(stat: &libc::stat) -> u64 {
        u64::try_from(stat.st_blocks)
            .unwrap_or(0)
            .saturating_mul(512)
    }

    fn open_child_directory(
        parent: &OwnedFd,
        name: &OsStr,
        expected: &libc::stat,
    ) -> Result<OwnedFd, &'static str> {
        let name = name_cstring(name)?;
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            return Err("directory-unreadable");
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let observed = stat_fd(&fd)?;
        if !is_directory(&observed) || stat_identity(&observed) != stat_identity(expected) {
            return Err("directory-identity-drift");
        }
        Ok(fd)
    }

    struct DirectoryNameIterator {
        stream: *mut libc::DIR,
        finished: bool,
    }

    impl DirectoryNameIterator {
        fn from_fd(fd: &OwnedFd) -> Result<Self, &'static str> {
            let duplicate = unsafe { libc::dup(fd.as_raw_fd()) };
            if duplicate < 0 {
                return Err("directory-unreadable");
            }
            let stream = unsafe { libc::fdopendir(duplicate) };
            if stream.is_null() {
                unsafe {
                    libc::close(duplicate);
                }
                return Err("directory-unreadable");
            }
            Ok(Self {
                stream,
                finished: false,
            })
        }
    }

    impl Iterator for DirectoryNameIterator {
        type Item = Result<OsString, ()>;

        fn next(&mut self) -> Option<Self::Item> {
            if self.finished {
                return None;
            }
            loop {
                unsafe {
                    *errno_location() = 0;
                }
                let entry = unsafe { libc::readdir(self.stream) };
                if entry.is_null() {
                    self.finished = true;
                    let errno = unsafe { *errno_location() };
                    return if errno == 0 { None } else { Some(Err(())) };
                }
                let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                return Some(Ok(OsString::from_vec(name.to_vec())));
            }
        }
    }

    impl Drop for DirectoryNameIterator {
        fn drop(&mut self) {
            if !self.stream.is_null() {
                unsafe {
                    libc::closedir(self.stream);
                }
                self.stream = std::ptr::null_mut();
            }
        }
    }

    pub(super) fn measure_root_with_resolution_hook<F>(
        root: &Path,
        max_entries: u64,
        max_duration: Duration,
        after_resolution: F,
    ) -> Result<AllocationMapEntry, String>
    where
        F: FnOnce(),
    {
        if !root.is_absolute() || max_entries == 0 || max_duration.is_zero() {
            return Err("allocation-map-options-invalid".into());
        }
        let started = Instant::now();
        let root_metadata = std::fs::symlink_metadata(root)
            .map_err(|_| "allocation-map-root-unavailable".to_string())?;
        if root_metadata.file_type().is_symlink() {
            return Err("allocation-map-root-symlink-rejected".into());
        }

        // Open the caller-selected object first. O_NOFOLLOW rejects a final symlink while
        // allowing the kernel to resolve intermediate links and `..` exactly once.
        let root_fd = open_bound_root(root).map_err(str::to_string)?;
        let root_stat = stat_fd(&root_fd).map_err(str::to_string)?;
        let resolved_root = std::fs::canonicalize(root)
            .map_err(|_| "allocation-map-root-unavailable".to_string())?;
        let resolved_metadata = std::fs::symlink_metadata(&resolved_root)
            .map_err(|_| "allocation-map-root-unavailable".to_string())?;
        if metadata_identity(&resolved_metadata) != stat_identity(&root_stat) {
            return Err("allocation-map-root-identity-drift".into());
        }

        // Tests use this seam to replace caller-visible pathnames after the descriptor has
        // been bound. Production never reopens those pathnames during traversal.
        after_resolution();

        let device = root_stat.st_dev as u64;
        let mut allocated_identities = HashSet::new();
        let mut allocated_total = 0_u64;
        let mut visited_entries = 1_u64;
        allocated_identities.insert(stat_identity(&root_stat));
        allocated_total = allocated_total.saturating_add(allocated_bytes(&root_stat));
        let mut stop_reason = None;
        let mut stack = Vec::new();
        if is_directory(&root_stat) {
            stack.push(root_fd);
        }

        'scan: while let Some(directory) = stack.pop() {
            if started.elapsed() >= max_duration {
                stop_reason = Some("duration-limit-reached");
                break;
            }
            let remaining_entries = max_entries.saturating_sub(visited_entries);
            let iterator = match DirectoryNameIterator::from_fd(&directory) {
                Ok(iterator) => iterator,
                Err(reason) => {
                    stop_reason = Some(reason);
                    break;
                }
            };
            let children = match collect_children_within_budget(
                iterator,
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

            for name in children.into_iter().rev() {
                if started.elapsed() >= max_duration {
                    stop_reason = Some("duration-limit-reached");
                    break 'scan;
                }
                let child_stat = match stat_child(&directory, &name) {
                    Ok(stat) => stat,
                    Err(reason) => {
                        stop_reason = Some(reason);
                        break 'scan;
                    }
                };
                if child_stat.st_dev as u64 != device {
                    continue;
                }
                visited_entries = visited_entries.saturating_add(1);
                if allocated_identities.insert(stat_identity(&child_stat)) {
                    allocated_total = allocated_total.saturating_add(allocated_bytes(&child_stat));
                }
                if is_directory(&child_stat) {
                    match open_child_directory(&directory, &name, &child_stat) {
                        Ok(child) => stack.push(child),
                        Err(reason) => {
                            stop_reason = Some(reason);
                            break 'scan;
                        }
                    }
                }
            }
        }

        Ok(AllocationMapEntry {
            root: root.to_string_lossy().into_owned(),
            allocated_bytes: allocated_total,
            visited_entries,
            classification: classification(&resolved_root),
            evidence_complete: stop_reason.is_none(),
            stop_reason,
        })
    }
}

/// Sum allocated filesystem blocks without following symlinks or crossing a device boundary.
#[cfg(unix)]
pub fn measure_root(
    root: &Path,
    max_entries: u64,
    max_duration: Duration,
) -> Result<AllocationMapEntry, String> {
    unix_bound::measure_root_with_resolution_hook(root, max_entries, max_duration, || {})
}

/// The descriptor-relative allocation map currently ships only where Unix directory handles
/// are available. Unsupported platforms fail closed rather than falling back to pathname races.
#[cfg(not(unix))]
pub fn measure_root(
    _root: &Path,
    _max_entries: u64,
    _max_duration: Duration,
) -> Result<AllocationMapEntry, String> {
    Err("allocation-map-platform-unsupported".into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::rc::Rc;

    fn measure_root_with_resolution_hook<F>(
        root: &Path,
        max_entries: u64,
        max_duration: Duration,
        after_resolution: F,
    ) -> Result<AllocationMapEntry, String>
    where
        F: FnOnce(),
    {
        unix_bound::measure_root_with_resolution_hook(
            root,
            max_entries,
            max_duration,
            after_resolution,
        )
    }

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
        use std::os::unix::fs::MetadataExt;
        let expected_without_target = std::fs::symlink_metadata(&root).unwrap().blocks() * 512
            + std::fs::symlink_metadata(root.join("link")).unwrap().blocks() * 512;
        assert_eq!(report.allocated_bytes, expected_without_target);
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
    fn traversal_remains_bound_to_the_root_resolved_before_link_retargeting() {
        use std::os::unix::fs::{symlink, MetadataExt};
        let temp = tempfile::tempdir().unwrap();
        let provider_parent = temp.path().join("Library");
        let provider_root = provider_parent.join("Mobile Documents");
        let ordinary_parent = temp.path().join("ordinary");
        let ordinary_root = ordinary_parent.join("Mobile Documents");
        std::fs::create_dir_all(&provider_root).unwrap();
        std::fs::create_dir_all(&ordinary_root).unwrap();
        std::fs::write(provider_root.join("provider.bin"), b"provider").unwrap();
        let bridge = temp.path().join("bridge");
        symlink(&provider_parent, &bridge).unwrap();
        let selected_root = bridge.join("Mobile Documents");

        let report = measure_root_with_resolution_hook(
            &selected_root,
            2,
            Duration::from_secs(1),
            || {
                std::fs::remove_file(&bridge).unwrap();
                symlink(&ordinary_parent, &bridge).unwrap();
            },
        )
        .unwrap();

        let expected = std::fs::symlink_metadata(&provider_root).unwrap().blocks() * 512
            + std::fs::symlink_metadata(provider_root.join("provider.bin"))
                .unwrap()
                .blocks()
                * 512;
        assert_eq!(report.root, selected_root.to_string_lossy());
        assert_eq!(report.classification, "provider-managed");
        assert_eq!(report.allocated_bytes, expected);
        assert_eq!(report.visited_entries, 2);
    }

    #[test]
    fn traversal_stays_bound_when_canonical_target_is_replaced_after_resolution() {
        use std::os::unix::fs::MetadataExt;
        let temp = tempfile::tempdir().unwrap();
        let provider_parent = temp.path().join("Library");
        let selected_root = provider_parent.join("Mobile Documents");
        let moved_root = temp.path().join("original-mobile-documents");
        std::fs::create_dir_all(&selected_root).unwrap();
        std::fs::write(selected_root.join("original.bin"), vec![0_u8; 4096]).unwrap();

        let report = measure_root_with_resolution_hook(
            &selected_root,
            2,
            Duration::from_secs(1),
            || {
                std::fs::rename(&selected_root, &moved_root).unwrap();
                std::fs::create_dir(&selected_root).unwrap();
                std::fs::write(selected_root.join("replacement.bin"), vec![0_u8; 1024 * 1024])
                    .unwrap();
            },
        )
        .unwrap();

        let expected = std::fs::symlink_metadata(&moved_root).unwrap().blocks() * 512
            + std::fs::symlink_metadata(moved_root.join("original.bin"))
                .unwrap()
                .blocks()
                * 512;
        assert_eq!(report.root, selected_root.to_string_lossy());
        assert_eq!(report.classification, "provider-managed");
        assert_eq!(report.allocated_bytes, expected);
        assert_eq!(report.visited_entries, 2);
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
