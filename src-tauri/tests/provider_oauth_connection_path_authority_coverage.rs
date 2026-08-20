use disksage_lib::provider_oauth::load_connections;
use std::path::Path;

#[test]
fn relative_connection_document_path_is_rejected_before_current_directory_resolution() {
    let relative = Path::new("disksage-oauth-relative-authority/connections.json");
    assert!(!relative.is_absolute());
    assert_eq!(
        load_connections(relative).unwrap_err(),
        "oauth-connection-document-path-invalid"
    );
}
