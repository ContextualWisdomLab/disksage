#[cfg(unix)]
mod unix_contract {
    use disksage_lib::colima_reclaim::plan_colima_reclaim;
    use std::{
        ffi::OsString,
        fs,
        os::unix::fs::PermissionsExt,
        path::Path,
        sync::Mutex,
        time::Duration,
    };

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvRestore {
        key: &'static str,
        value: Option<OsString>,
    }

    impl EnvRestore {
        fn capture(key: &'static str) -> Self {
            Self {
                key,
                value: std::env::var_os(key),
            }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            match self.value.take() {
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    fn planned_with(cache_root: &Path) -> disksage_lib::colima_reclaim::ColimaReclaimPlan {
        let temp = tempfile::tempdir().expect("temporary directory");
        let executable = temp.path().join("colima");
        fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = list ]; then exit 0; fi\nexit 2\n",
        )
        .expect("fake colima");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .expect("executable mode");
        plan_colima_reclaim(&executable, cache_root, Duration::from_secs(1))
    }

    #[test]
    fn relative_colima_cache_home_never_authorizes_prune() {
        let _guard = ENV_LOCK.lock().expect("environment test lock");
        let restore = EnvRestore::capture("COLIMA_CACHE_HOME");
        unsafe { std::env::set_var("COLIMA_CACHE_HOME", "relative-colima-cache") };

        let temp = tempfile::tempdir().expect("temporary directory");
        let cache_root = temp.path().join("cache");
        fs::create_dir(&cache_root).expect("cache directory");
        fs::write(cache_root.join("asset"), vec![1u8; 8192]).expect("cache fixture");

        let plan = planned_with(&cache_root);
        drop(restore);

        assert!(!plan.evidence_complete);
        assert!(plan
            .issues
            .iter()
            .any(|issue| issue == "colima-cache-home-relative-unsupported"));
        assert!(plan.plan_fingerprint.is_none());
        assert!(plan.cache_prune_approval_phrase.is_none());
    }

    #[test]
    fn explicit_colima_cache_home_must_match_measured_root() {
        let _guard = ENV_LOCK.lock().expect("environment test lock");
        let restore = EnvRestore::capture("COLIMA_CACHE_HOME");
        let configured = tempfile::tempdir().expect("configured cache home");
        unsafe { std::env::set_var("COLIMA_CACHE_HOME", configured.path()) };

        let measured = tempfile::tempdir().expect("measured cache root");
        fs::write(measured.path().join("asset"), vec![1u8; 8192]).expect("cache fixture");

        let plan = planned_with(measured.path());
        drop(restore);

        assert!(!plan.evidence_complete);
        assert!(plan
            .issues
            .iter()
            .any(|issue| issue == "colima-cache-root-mismatch"));
        assert!(plan.plan_fingerprint.is_none());
        assert!(plan.cache_prune_approval_phrase.is_none());
    }

    #[test]
    fn path_installed_colima_is_resolved_before_identity_checks() {
        let _guard = ENV_LOCK.lock().expect("environment test lock");
        let cache_restore = EnvRestore::capture("COLIMA_CACHE_HOME");
        let path_restore = EnvRestore::capture("PATH");

        let temp = tempfile::tempdir().expect("temporary directory");
        let bin_dir = temp.path().join("bin");
        let cache_root = temp.path().join("cache");
        fs::create_dir(&bin_dir).expect("bin directory");
        fs::create_dir(&cache_root).expect("cache directory");
        fs::write(cache_root.join("asset"), vec![1u8; 8192]).expect("cache fixture");

        let executable = bin_dir.join("colima");
        fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = list ] && [ \"$2\" = --json ]; then\n  printf '%s\\n' '{\"name\":\"default\",\"status\":\"Stopped\",\"runtime\":\"docker\",\"disk\":1073741824}'\n  exit 0\nfi\nexit 2\n",
        )
        .expect("fake PATH colima");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .expect("executable mode");

        unsafe {
            std::env::set_var("PATH", &bin_dir);
            std::env::set_var("COLIMA_CACHE_HOME", &cache_root);
        }

        let plan = plan_colima_reclaim(Path::new("colima"), &cache_root, Duration::from_secs(1));
        drop(path_restore);
        drop(cache_restore);

        assert!(plan.executable_available, "issues: {:?}", plan.issues);
        assert!(plan.evidence_complete, "issues: {:?}", plan.issues);
        assert_eq!(plan.profiles.len(), 1);
        assert_eq!(plan.profiles[0].name, "default");
        assert!(plan.plan_fingerprint.is_some());
        assert!(plan.cache_prune_approval_phrase.is_some());
    }
}
