#[cfg(unix)]
mod unix_contract {
    use disksage_lib::colima_reclaim::plan_colima_reclaim;
    use std::{
        ffi::OsString,
        fs,
        os::unix::fs::PermissionsExt,
        sync::Mutex,
        time::Duration,
    };

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct CacheHomeRestore(Option<OsString>);

    impl Drop for CacheHomeRestore {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => unsafe { std::env::set_var("COLIMA_CACHE_HOME", value) },
                None => unsafe { std::env::remove_var("COLIMA_CACHE_HOME") },
            }
        }
    }

    #[test]
    fn relative_colima_cache_home_never_authorizes_prune() {
        let _guard = ENV_LOCK.lock().expect("environment test lock");
        let restore = CacheHomeRestore(std::env::var_os("COLIMA_CACHE_HOME"));
        unsafe { std::env::set_var("COLIMA_CACHE_HOME", "relative-colima-cache") };

        let temp = tempfile::tempdir().expect("temporary directory");
        let cache_root = temp.path().join("cache");
        fs::create_dir(&cache_root).expect("cache directory");
        fs::write(cache_root.join("asset"), vec![1u8; 8192]).expect("cache fixture");

        let executable = temp.path().join("colima");
        fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = list ]; then exit 0; fi\nexit 2\n",
        )
        .expect("fake colima");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .expect("executable mode");

        let plan = plan_colima_reclaim(&executable, &cache_root, Duration::from_secs(1));
        drop(restore);

        assert!(!plan.evidence_complete);
        assert!(plan
            .issues
            .iter()
            .any(|issue| issue == "colima-cache-home-relative-unsupported"));
        assert!(plan.plan_fingerprint.is_none());
        assert!(plan.cache_prune_approval_phrase.is_none());
    }
}
