//! Fail-closed public coverage for CloudDocs SQLite URI admission.
//!
//! The probe must never reinterpret hostile filesystem bytes as SQLite URI syntax. These fixtures
//! force the consistent-snapshot probe to fall back, then prove the original managed database path
//! is rejected before sqlite3 can consume an unsafe or non-Unicode URI.

use disksage_lib::icloud_sync_health::probe_icloud_sync_health;

#[test]
fn uri_metacharacter_in_database_path_is_rejected_without_source_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let db_dir = temp.path().join("cloud#docs-db");
    std::fs::create_dir(&db_dir).unwrap();
    let client_db = db_dir.join("client.db");
    let original = b"not-a-sqlite-database-private-fixture";
    std::fs::write(&client_db, original).unwrap();

    let error = probe_icloud_sync_health(&db_dir, 42).unwrap_err();

    assert_eq!(error, "icloud-sync-health-client-db-uri-unsafe");
    assert_eq!(std::fs::read(&client_db).unwrap(), original);
    assert!(!db_dir.join("client.db-shm").exists());
    assert!(!db_dir.join("client.db-wal").exists());
}

#[cfg(unix)]
#[test]
fn non_utf8_database_path_is_rejected_without_source_mutation() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp = tempfile::tempdir().unwrap();
    let mut name = b"cloud-docs-".to_vec();
    name.push(0xff);
    let db_dir = temp.path().join(OsString::from_vec(name));
    std::fs::create_dir(&db_dir).unwrap();
    let client_db = db_dir.join("client.db");
    let original = b"not-a-sqlite-database-private-fixture";
    std::fs::write(&client_db, original).unwrap();

    let error = probe_icloud_sync_health(&db_dir, 42).unwrap_err();

    assert_eq!(error, "icloud-sync-health-client-db-not-unicode");
    assert_eq!(std::fs::read(&client_db).unwrap(), original);
    assert!(!db_dir.join("client.db-shm").exists());
    assert!(!db_dir.join("client.db-wal").exists());
}
