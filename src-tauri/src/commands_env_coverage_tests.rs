//! Coverage for command-layer environment branches that must remain deterministic and side-effect safe.

#[cfg(not(windows))]
use crate::commands::list_roots;

#[cfg(not(windows))]
struct EnvRestore {
    key: &'static str,
    value: Option<std::ffi::OsString>,
}

#[cfg(not(windows))]
impl EnvRestore {
    fn remove(key: &'static str) -> Self {
        let value = std::env::var_os(key);
        std::env::remove_var(key);
        Self { key, value }
    }
}

#[cfg(not(windows))]
impl Drop for EnvRestore {
    fn drop(&mut self) {
        match self.value.take() {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

#[cfg(not(windows))]
#[test]
fn list_roots_does_not_invent_a_home_root_when_home_is_absent() {
    // The Rust test workflow is serialized (`RUST_TEST_THREADS=1`), so temporarily removing HOME
    // cannot race another DiskSage unit test. Restore it even if the assertion unwinds.
    let _restore = EnvRestore::remove("HOME");

    let roots = list_roots();

    assert_eq!(roots, vec!["/".to_string()]);
}
