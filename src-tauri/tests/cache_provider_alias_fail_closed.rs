#[cfg(unix)]
mod cloud {
    use std::path::Path;

    pub(crate) fn path_inside_managed_file_provider_storage(path: &Path) -> bool {
        let mut previous = String::new();
        path.components().any(|component| {
            let name = component
                .as_os_str()
                .to_string_lossy()
                .trim()
                .to_lowercase();
            let managed = name == "file provider storage"
                || (previous == "library"
                    && matches!(name.as_str(), "mobile documents" | "cloudstorage"))
                || (previous == "application support" && name == "fileprovider");
            previous = name;
            managed
        })
    }
}

#[cfg(unix)]
mod safety {
    use std::path::Path;

    pub(crate) fn filesystem_object_id(path: &Path) -> std::io::Result<String> {
        Ok(path.to_string_lossy().into_owned())
    }
}

#[cfg(unix)]
#[path = "../src/rules.rs"]
mod production_rules;

#[cfg(unix)]
mod provider_alias_contract {
    use super::production_rules;
    use std::fs;
    use std::path::Path;

    #[test]
    fn symlinked_ancestor_into_managed_provider_storage_never_gets_cleanup_authority() {
        let temp = tempfile::tempdir().expect("temp root");
        let home = temp.path().join("home");
        let managed_parent = home
            .join("Library")
            .join("CloudStorage")
            .join("Provider");
        let managed_cache = managed_parent.join("cache");
        fs::create_dir_all(&managed_cache).expect("managed cache fixture");
        fs::write(managed_cache.join("customer-owned.bin"), b"keep")
            .expect("managed payload fixture");

        let alias_parent = temp.path().join("cache-alias");
        std::os::unix::fs::symlink(&managed_parent, &alias_parent).expect("ancestor alias");
        let aliased_cache = alias_parent.join("cache");
        let bases = production_rules::BaseDirs {
            temp: aliased_cache.clone(),
            local_data: temp.path().join("local"),
            home,
        };

        let candidate = production_rules::cache_candidates(&bases)
            .into_iter()
            .find(|candidate| candidate.id == "os-temp")
            .expect("os-temp catalog entry");
        assert!(
            !candidate.exists,
            "a cache reached through a symlinked ancestor into managed provider storage must not be actionable"
        );
        assert_eq!(candidate.bytes, 0);
        assert!(!production_rules::is_catalog_path(&bases, Path::new(&aliased_cache)));
        assert!(production_rules::clean_targets(&aliased_cache).is_empty());
    }
}
