use disksage_lib::provider_oauth::{connections_path, load_connections};
use std::fs;
use std::io::Write;

fn write_json(path: &std::path::Path, value: &serde_json::Value) {
    let mut file = fs::File::create(path).unwrap();
    serde_json::to_writer(&mut file, value).unwrap();
    file.flush().unwrap();
}

fn valid_connection_json() -> serde_json::Value {
    serde_json::json!({
        "connection_id": "20ce9ca07d014bcf578cd0e494f9278fa2ad69e7e62e5dbd9afc5fe30bf7e7eb",
        "provider": "onedrive",
        "cloud_root_id": "root",
        "cloud_root_path": "/tmp/root",
        "client_id": "12345678-1234-1234-1234-123456789abc",
        "scope": "Files.Read offline_access",
        "connected_at_ms": 1
    })
}

fn write_single_connection(path: &std::path::Path, connection: serde_json::Value) {
    write_json(
        path,
        &serde_json::json!({
            "version": 1,
            "connections": [connection]
        }),
    );
}

fn assert_single_connection_error(
    path: &std::path::Path,
    connection: serde_json::Value,
    expected: &str,
) {
    write_single_connection(path, connection);
    assert_eq!(load_connections(path).unwrap_err(), expected);
}

#[test]
fn real_filesystem_connection_document_rejects_non_regular_and_oversized_inputs() {
    let directory = tempfile::tempdir().unwrap();
    let document_path = connections_path(directory.path());

    fs::create_dir(&document_path).unwrap();
    assert_eq!(
        load_connections(&document_path).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
    fs::remove_dir(&document_path).unwrap();

    let oversized = fs::File::create(&document_path).unwrap();
    oversized.set_len(256 * 1024 + 1).unwrap();
    drop(oversized);
    assert_eq!(
        load_connections(&document_path).unwrap_err(),
        "oauth-connection-document-too-large"
    );
}

#[cfg(unix)]
#[test]
fn real_filesystem_connection_document_rejects_symlink_identity() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target.json");
    fs::write(&target, br#"{"version":1,"connections":[]}"#).unwrap();
    let document_path = connections_path(directory.path());
    symlink(&target, &document_path).unwrap();

    assert_eq!(
        load_connections(&document_path).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
}

#[test]
fn connection_document_parser_fails_closed_for_invalid_schema_and_connection_identity() {
    let directory = tempfile::tempdir().unwrap();
    let document_path = connections_path(directory.path());

    fs::write(&document_path, b"not-json").unwrap();
    assert_eq!(
        load_connections(&document_path).unwrap_err(),
        "oauth-connection-document-invalid"
    );

    write_json(
        &document_path,
        &serde_json::json!({"version": 2, "connections": []}),
    );
    assert_eq!(
        load_connections(&document_path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    write_json(
        &document_path,
        &serde_json::json!({
            "version": 1,
            "connections": (0..33).map(|_| valid_connection_json()).collect::<Vec<_>>()
        }),
    );
    assert_eq!(
        load_connections(&document_path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    let mut invalid_connection = valid_connection_json();
    invalid_connection["connection_id"] = serde_json::Value::String("not-a-sha256".to_string());
    assert_single_connection_error(
        &document_path,
        invalid_connection,
        "oauth-connection-invalid",
    );
}

#[test]
fn connection_document_validation_covers_identity_path_scope_and_client_boundaries() {
    let directory = tempfile::tempdir().unwrap();
    let document_path = connections_path(directory.path());

    write_single_connection(&document_path, valid_connection_json());
    let connections = load_connections(&document_path).unwrap();
    assert_eq!(connections.len(), 1);
    assert_eq!(
        connections[0].connection_id,
        "20ce9ca07d014bcf578cd0e494f9278fa2ad69e7e62e5dbd9afc5fe30bf7e7eb"
    );

    let mut non_hex_identity = valid_connection_json();
    non_hex_identity["connection_id"] = serde_json::Value::String("g".repeat(64));
    assert_single_connection_error(
        &document_path,
        non_hex_identity,
        "oauth-connection-invalid",
    );

    let mut blank_root_id = valid_connection_json();
    blank_root_id["cloud_root_id"] = serde_json::Value::String(" ".to_string());
    assert_single_connection_error(
        &document_path,
        blank_root_id,
        "oauth-connection-invalid",
    );

    let mut blank_root_path = valid_connection_json();
    blank_root_path["cloud_root_path"] = serde_json::Value::String("\t".to_string());
    assert_single_connection_error(
        &document_path,
        blank_root_path,
        "oauth-connection-invalid",
    );

    let mut relative_root_path = valid_connection_json();
    relative_root_path["cloud_root_path"] =
        serde_json::Value::String("relative/root".to_string());
    assert_single_connection_error(
        &document_path,
        relative_root_path,
        "oauth-connection-invalid",
    );

    let mut invalid_scope = valid_connection_json();
    invalid_scope["scope"] = serde_json::Value::String("Files.ReadWrite".to_string());
    assert_single_connection_error(&document_path, invalid_scope, "oauth-connection-invalid");

    let mut mismatched_identity = valid_connection_json();
    mismatched_identity["connection_id"] = serde_json::Value::String("0".repeat(64));
    assert_single_connection_error(
        &document_path,
        mismatched_identity,
        "oauth-connection-id-mismatch",
    );

    let mut provider_format_invalid = valid_connection_json();
    provider_format_invalid["client_id"] = serde_json::Value::String("not-a-guid".to_string());
    assert_single_connection_error(
        &document_path,
        provider_format_invalid,
        "oauth-client-id-provider-format-invalid",
    );
}

#[test]
fn connection_document_validation_covers_client_id_safety_guards() {
    let directory = tempfile::tempdir().unwrap();
    let document_path = connections_path(directory.path());

    for invalid_client_id in [
        String::new(),
        "a".repeat(513),
        " 12345678-1234-1234-1234-123456789abc".to_string(),
        "é".to_string(),
        "abc\n".to_string(),
    ] {
        let mut connection = valid_connection_json();
        connection["client_id"] = serde_json::Value::String(invalid_client_id);
        assert_single_connection_error(
            &document_path,
            connection,
            "oauth-client-id-invalid",
        );
    }
}
