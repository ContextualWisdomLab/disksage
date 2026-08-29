use std::path::PathBuf;
use std::sync::Mutex;

use crate::rules::{cache_catalog_path, BaseDirs};

static GRADLE_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn gradle_catalog_honors_absolute_gradle_user_home() {
    let _guard = GRADLE_ENV_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let custom_gradle_home = temp.path().join("custom-gradle-home");
    let original = std::env::var_os("GRADLE_USER_HOME");
    std::env::set_var("GRADLE_USER_HOME", &custom_gradle_home);

    let bases = BaseDirs {
        temp: temp.path().join("tmp"),
        local_data: temp.path().join("cache"),
        home: temp.path().join("home"),
    };

    assert_eq!(
        cache_catalog_path(&bases, "gradle-cache"),
        Some(custom_gradle_home.join("caches"))
    );
    assert_eq!(
        cache_catalog_path(&bases, "gradle-wrapper-cache"),
        Some(custom_gradle_home.join("wrapper").join("dists"))
    );
    assert_eq!(
        cache_catalog_path(&bases, "gradle-jdk-cache"),
        Some(custom_gradle_home.join("jdks"))
    );
    assert_eq!(
        cache_catalog_path(&bases, "gradle-daemon-cache"),
        Some(custom_gradle_home.join("daemon"))
    );

    match original {
        Some(value) => std::env::set_var("GRADLE_USER_HOME", value),
        None => std::env::remove_var("GRADLE_USER_HOME"),
    }
}

#[test]
fn relative_gradle_user_home_fails_closed_to_default_home() {
    let _guard = GRADLE_ENV_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let original = std::env::var_os("GRADLE_USER_HOME");
    std::env::set_var("GRADLE_USER_HOME", PathBuf::from("relative-gradle-home"));

    let bases = BaseDirs {
        temp: temp.path().join("tmp"),
        local_data: temp.path().join("cache"),
        home: temp.path().join("home"),
    };

    assert_eq!(
        cache_catalog_path(&bases, "gradle-cache"),
        Some(bases.home.join(".gradle").join("caches"))
    );

    match original {
        Some(value) => std::env::set_var("GRADLE_USER_HOME", value),
        None => std::env::remove_var("GRADLE_USER_HOME"),
    }
}
