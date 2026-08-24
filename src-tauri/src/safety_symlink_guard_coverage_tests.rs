//! Filesystem-symlink regressions for the public move safety boundary.
//!
//! These tests exercise real Unix symlinks through `move_file` so a caller-controlled alias to a
//! protected system tree cannot turn a seemingly local destination or source into a protected
//! mutation target. The guard must fail before creating a destination or writing mutation audit
//! state.

#[cfg(unix)]
mod unix {
    use crate::safety::{move_file, SafetyError};
    use std::os::unix::fs::symlink;

    #[test]
    fn move_rejects_missing_destination_beneath_symlinked_system_parent() {
        let root = tempfile::tempdir().expect("temporary move root");
        let source = root.path().join("source.bin");
        let system_alias = root.path().join("system-alias");
        let destination = system_alias.join("disksage-must-not-create.bin");
        let journal = root.path().join("move-journal.jsonl");
        std::fs::write(&source, b"reviewed-source").expect("write source fixture");
        symlink("/usr", &system_alias).expect("create protected-system symlink fixture");

        let error = move_file(&source, &destination, &journal, 40_001).unwrap_err();

        assert!(matches!(error, SafetyError::Protected(_)));
        assert_eq!(std::fs::read(&source).unwrap(), b"reviewed-source");
        assert!(
            !journal.exists(),
            "symlinked protected destination must fail before mutation journaling"
        );
    }

    #[test]
    fn move_rejects_existing_source_symlink_that_resolves_into_system_tree() {
        let root = tempfile::tempdir().expect("temporary move root");
        let protected_alias = root.path().join("protected-source");
        let destination = root.path().join("destination");
        let journal = root.path().join("move-journal.jsonl");
        symlink("/usr/bin", &protected_alias).expect("create protected source symlink fixture");

        let error = move_file(&protected_alias, &destination, &journal, 40_002).unwrap_err();

        assert!(matches!(error, SafetyError::Protected(_)));
        assert!(protected_alias.exists(), "the caller-supplied symlink must remain intact");
        assert!(!destination.exists());
        assert!(
            !journal.exists(),
            "protected source aliases must fail before mutation journaling"
        );
    }
}
