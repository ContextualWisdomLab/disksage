use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_api_write::{delete_uploaded_object, upload_file};
use std::path::Path;

#[test]
fn unsupported_icloud_upload_fails_before_source_or_network_access() {
    let error = upload_file(
        CloudProvider::Icloud,
        Path::new("local-root"),
        Path::new("local-root/archive.bin"),
        Path::new("source-does-not-exist"),
        42,
        "token_1",
    )
    .unwrap_err();

    assert_eq!(error, "provider-api-icloud-unsupported");
}

#[test]
fn malformed_bearer_tokens_fail_before_provider_dispatch() {
    let oversized_token = "a".repeat(64 * 1024 + 1);
    for token in ["", "line\nbreak", oversized_token.as_str()] {
        let error = upload_file(
            CloudProvider::Icloud,
            Path::new("local-root"),
            Path::new("local-root/archive.bin"),
            Path::new("source-does-not-exist"),
            42,
            token,
        )
        .unwrap_err();

        assert_eq!(error, "provider-api-bearer-token-invalid");
    }
}

#[test]
fn malformed_object_ids_and_unsupported_delete_fail_before_transport() {
    for object_id in ["", "   ", "line\nbreak"] {
        assert_eq!(
            delete_uploaded_object(CloudProvider::Icloud, object_id, "token_1").unwrap_err(),
            "provider-api-object-id-invalid"
        );
    }

    assert_eq!(
        delete_uploaded_object(CloudProvider::Icloud, "object-1", "token_1").unwrap_err(),
        "provider-api-icloud-unsupported"
    );
}
