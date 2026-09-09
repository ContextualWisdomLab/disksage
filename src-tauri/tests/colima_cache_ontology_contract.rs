#[cfg(unix)]
mod unix_contract {
    use disksage_lib::colima_reclaim::plan_colima_reclaim;
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

    #[test]
    fn cache_reclaim_plan_uses_download_cache_ontology_class() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let executable = temp.path().join("colima");
        let cache_root = temp.path().join("cache");
        fs::create_dir(&cache_root).expect("cache directory");
        fs::write(cache_root.join("asset"), vec![1u8; 8192]).expect("cache fixture");
        fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = list ] && [ \"$2\" = --json ]; then exit 0; fi\nexit 2\n",
        )
        .expect("fake colima");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .expect("executable mode");

        let plan = plan_colima_reclaim(&executable, &cache_root, Duration::from_secs(1));

        assert_eq!(
            plan.ontology_class,
            "https://disksage.app/ontology#ColimaDownloadCache"
        );
    }
}
