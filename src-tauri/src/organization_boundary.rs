//! Preserve package and companion-file relationships before individual organization moves.
use std::path::Path;

/// A package descendant must not become an independently movable document.
pub fn package_ancestor(path: &Path) -> bool {
    path.ancestors().any(|part| {
        part.extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| {
                [
                    "app",
                    "bundle",
                    "framework",
                    "photoslibrary",
                    "xcodeproj",
                    "xcworkspace",
                ]
                .iter()
                .any(|suffix| value.eq_ignore_ascii_case(suffix))
            })
    })
}

/// Retain descendants of recognized projects, including worktree marker files.
pub fn validate_project_ancestors(directory: &Path) -> Result<(), String> {
    let resolved = std::fs::canonicalize(directory)
        .map_err(|_| "프로젝트 경계를 확인할 수 없어 이동을 보류합니다.")?;
    for ancestor in resolved.ancestors() {
        for marker in [
            ".git",
            ".hg",
            ".svn",
            "Cargo.toml",
            "package.json",
            "pyproject.toml",
            "go.mod",
            "CMakeLists.txt",
            "wscript",
            "SConstruct",
            "configure.ac",
        ] {
            match std::fs::symlink_metadata(ancestor.join(marker)) {
                Ok(_) => {
                    return Err(
                        "프로젝트 내부 자료는 기존 관계를 보존하기 위해 따로 옮기지 않습니다."
                            .into(),
                    )
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("프로젝트 경계를 확인할 수 없어 이동을 보류합니다.".into()),
            }
        }
    }
    Ok(())
}

/// Shared basename is only a preservation hint, never evidence of duplicate content.
pub fn companion_paths(left: &Path, right: &Path) -> bool {
    left != right
        && left.parent() == right.parent()
        && left.file_stem().is_some()
        && left.file_stem() == right.file_stem()
        && left.extension() != right.extension()
}

/// Reject destinations inside packages, including aliases through an existing ancestor.
pub fn validate_destination(path: &Path) -> Result<(), String> {
    if package_ancestor(path) {
        return Err("organize-destination-package-boundary".into());
    }
    for ancestor in path.ancestors().skip(1) {
        match std::fs::symlink_metadata(ancestor) {
            Ok(_) => {
                let resolved = std::fs::canonicalize(ancestor)
                    .map_err(|_| "organize-destination-unavailable")?;
                if package_ancestor(&resolved) {
                    return Err("organize-destination-package-boundary".into());
                }
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("organize-destination-unavailable".into()),
        }
    }
    Err("organize-destination-unavailable".into())
}

/// Recheck siblings at execution: a bounded scan snapshot may omit a companion.
/// An unreadable directory cannot establish that moving one member is safe.
pub fn validate_individual_move(path: &Path) -> Result<(), String> {
    if package_ancestor(path) {
        return Err("organize-package-boundary".into());
    }
    let resolved = std::fs::canonicalize(path).map_err(|_| "organize-source-unavailable")?;
    if package_ancestor(&resolved) {
        return Err("organize-package-boundary".into());
    }
    let parent = path.parent().ok_or("organize-parent-unavailable")?;
    validate_project_ancestors(resolved.parent().ok_or("organize-parent-unavailable")?)?;
    let siblings = std::fs::read_dir(parent).map_err(|_| "organize-parent-unavailable")?;
    for (index, sibling) in siblings.enumerate() {
        // ponytail: refuse oversized sibling sets; a bundle-aware planner can handle them later.
        if index >= 10_000 {
            return Err("organize-sibling-scope-incomplete".into());
        }
        let sibling = sibling.map_err(|_| "organize-sibling-unavailable")?;
        if companion_paths(path, &sibling.path()) {
            return Err("organize-companion-bundle-required".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_project_marker_retains_individual_document() {
        let root = std::env::temp_dir().join(format!("disksage-project-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let nested = root.join("documents");
        std::fs::create_dir(&nested).unwrap();
        let file = nested.join("note.txt");
        std::fs::write(&file, b"keep project relationships").unwrap();
        assert!(validate_individual_move(&file).is_ok());
        let marker = root.join(".git");
        std::fs::write(&marker, b"gitdir: elsewhere").unwrap();
        let result = validate_individual_move(&file);
        #[cfg(unix)]
        {
            let alias =
                std::env::temp_dir().join(format!("disksage-project-alias-{}", std::process::id()));
            std::os::unix::fs::symlink(&nested, &alias).unwrap();
            let aliased = validate_individual_move(&alias.join("note.txt"));
            std::fs::remove_file(&alias).unwrap();
            assert!(aliased.is_err());
        }
        std::fs::remove_file(&marker).unwrap();
        std::fs::remove_file(&file).unwrap();
        std::fs::remove_dir(&nested).unwrap();
        std::fs::remove_dir(&root).unwrap();
        assert!(
            result.is_err(),
            "a late project marker must retain the document"
        );
    }

    #[cfg(unix)]
    #[test]
    fn package_alias_cannot_bypass_preservation() {
        let root =
            std::env::temp_dir().join(format!("disksage-package-alias-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let package = root.join("Editor.app");
        std::fs::create_dir(&package).unwrap();
        let file = package.join("document.txt");
        std::fs::write(&file, b"retain").unwrap();
        let alias = root.join("ordinary-folder");
        std::os::unix::fs::symlink(&package, &alias).unwrap();
        assert_eq!(
            validate_individual_move(&alias.join("document.txt")).unwrap_err(),
            "organize-package-boundary"
        );
        assert_eq!(
            validate_destination(&alias.join("new/sub/document.txt")).unwrap_err(),
            "organize-destination-package-boundary"
        );
        assert!(validate_destination(&root.join("ordinary/new/document.txt")).is_ok());
        assert_eq!(std::fs::read(&file).unwrap(), b"retain");
        std::fs::remove_file(alias).unwrap();
        std::fs::remove_file(file).unwrap();
        std::fs::remove_dir(package).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn package_descendants_and_companion_roles_are_preserved() {
        assert!(package_ancestor(Path::new(
            "/a/Editor.APP/Contents/document.txt"
        )));
        assert!(!package_ancestor(Path::new(
            "/a/Editor.app-notes/document.txt"
        )));
        assert!(companion_paths(
            Path::new("/a/recording.wav"),
            Path::new("/a/recording.tmk")
        ));
        assert!(companion_paths(
            Path::new("/a/transcript.json"),
            Path::new("/a/transcript.txt")
        ));
        assert!(!companion_paths(
            Path::new("/a/transcript.txt"),
            Path::new("/b/transcript.json")
        ));
        assert!(!companion_paths(
            Path::new("/a/transcript.txt"),
            Path::new("/a/transcript.txt")
        ));
        let root = std::env::temp_dir().join(format!("disksage-bundle-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let original = root.join("recording.wav");
        std::fs::write(&original, b"preserve original").unwrap();
        assert!(validate_individual_move(&original).is_ok());
        let companion = root.join("recording.tmk");
        std::fs::write(&companion, b"preserve markers").unwrap();
        assert_eq!(
            validate_individual_move(&original).unwrap_err(),
            "organize-companion-bundle-required"
        );
        assert_eq!(std::fs::read(&original).unwrap(), b"preserve original");
        std::fs::remove_file(companion).unwrap();
        std::fs::remove_file(original).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
