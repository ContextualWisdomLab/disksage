use std::path::Path;

use crate::{commands::CleanResult, rules};

const ATOMIC_TRASH_UNAVAILABLE: &str = "cache-cleanup-atomic-trash-unavailable";

fn clean_cache_contents_inner(
    bases: &rules::BaseDirs,
    dir: &Path,
) -> Result<Vec<CleanResult>, String> {
    if !rules::is_catalog_path(bases, dir) {
        return Err("cache-root-not-current-or-safe".into());
    }

    // A path-based recycle-bin API cannot preserve the identity of a child entry across the
    // final same-user rename/symlink race on every supported desktop platform. Re-validating the
    // root immediately before a path-based delete still leaves a check/use window. Until the
    // recycle operation itself is bound to the validated filesystem object, refuse cache
    // mutation instead of risking moving an unrelated path to the trash.
    Err(ATOMIC_TRASH_UNAVAILABLE.into())
}

/// Validate an approved cache root and fail closed until DiskSage has a recycle operation that is
/// bound to the exact validated filesystem object. Read-only cache discovery remains available;
/// this command deliberately grants no destructive authority while path identity can race.
#[cfg(not(coverage))]
#[tauri::command]
pub fn clean_cache_contents(dir: String) -> Result<Vec<CleanResult>, String> {
    let bases = rules::BaseDirs::from_env().ok_or("cache-base-directories-unavailable")?;
    clean_cache_contents_inner(&bases, Path::new(&dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fake_bases(root: &Path) -> rules::BaseDirs {
        rules::BaseDirs {
            temp: root.join("cache"),
            local_data: root.join("local"),
            home: root.join("home"),
        }
    }

    #[test]
    fn cleanup_rejects_non_catalog_root() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        fs::create_dir(&bases.temp).unwrap();

        let error = clean_cache_contents_inner(&bases, tmp.path())
            .err()
            .expect("non-catalog root should be rejected");

        assert_eq!(error, "cache-root-not-current-or-safe");
    }

    #[test]
    fn cleanup_refuses_mutation_until_target_identity_can_be_preserved() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        fs::create_dir(&bases.temp).unwrap();
        let victim = bases.temp.join("keep.bin");
        fs::write(&victim, b"keep").unwrap();

        let error = clean_cache_contents_inner(&bases, &bases.temp)
            .err()
            .expect("path-based cache cleanup must fail closed");

        assert_eq!(error, ATOMIC_TRASH_UNAVAILABLE);
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
        assert!(bases.temp.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_rejects_symlinked_catalog_root_without_touching_outside_data() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let outside_file = outside.join("outside.bin");
        fs::write(&outside_file, b"outside").unwrap();
        std::os::unix::fs::symlink(&outside, &bases.temp).unwrap();

        let error = clean_cache_contents_inner(&bases, &bases.temp)
            .err()
            .expect("symlinked catalog root should be rejected");

        assert_eq!(error, "cache-root-not-current-or-safe");
        assert_eq!(fs::read(&outside_file).unwrap(), b"outside");
    }
}
