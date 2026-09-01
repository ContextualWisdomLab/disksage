#[cfg(unix)]
mod unix_contract {
    use disksage_lib::colima_platform::configured_cache_root;
    use std::{ffi::OsString, path::Path, sync::Mutex};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvRestore {
        colima: Option<OsString>,
        xdg: Option<OsString>,
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            match self.colima.take() {
                Some(value) => unsafe { std::env::set_var("COLIMA_CACHE_HOME", value) },
                None => unsafe { std::env::remove_var("COLIMA_CACHE_HOME") },
            }
            match self.xdg.take() {
                Some(value) => unsafe { std::env::set_var("XDG_CACHE_HOME", value) },
                None => unsafe { std::env::remove_var("XDG_CACHE_HOME") },
            }
        }
    }

    fn restore() -> EnvRestore {
        EnvRestore {
            colima: std::env::var_os("COLIMA_CACHE_HOME"),
            xdg: std::env::var_os("XDG_CACHE_HOME"),
        }
    }

    #[test]
    fn xdg_cache_home_matches_colima_upstream_cache_contract() {
        let _guard = ENV_LOCK.lock().expect("environment test lock");
        let restore = restore();
        unsafe { std::env::remove_var("COLIMA_CACHE_HOME") };
        let xdg = tempfile::tempdir().expect("xdg cache home");
        unsafe { std::env::set_var("XDG_CACHE_HOME", xdg.path()) };

        let fallback = Path::new("/fallback/cache");
        let actual = configured_cache_root(fallback).expect("configured cache root");
        drop(restore);

        assert_eq!(actual, xdg.path().join("colima"));
    }

    #[test]
    fn relative_explicit_cache_configuration_fails_closed() {
        let _guard = ENV_LOCK.lock().expect("environment test lock");
        let restore = restore();
        unsafe { std::env::set_var("COLIMA_CACHE_HOME", "relative-colima-cache") };
        unsafe { std::env::remove_var("XDG_CACHE_HOME") };

        let error = configured_cache_root(Path::new("/fallback/cache"))
            .expect_err("relative Colima cache home must not be authorized");
        drop(restore);

        assert_eq!(error, "colima-cache-home-relative-unsupported");
    }

    #[test]
    fn platform_cache_directory_is_used_only_without_explicit_configuration() {
        let _guard = ENV_LOCK.lock().expect("environment test lock");
        let restore = restore();
        unsafe { std::env::remove_var("COLIMA_CACHE_HOME") };
        unsafe { std::env::remove_var("XDG_CACHE_HOME") };

        let fallback = Path::new("/platform/cache");
        let actual = configured_cache_root(fallback).expect("platform cache root");
        drop(restore);

        assert_eq!(actual, fallback.join("colima"));
    }
}
