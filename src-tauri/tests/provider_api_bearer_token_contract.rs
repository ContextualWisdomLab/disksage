use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_api_write::{delete_uploaded_object, upload_file};
use std::path::Path;

fn upload_error(token: &str) -> String {
    upload_file(
        CloudProvider::Icloud,
        Path::new("local-root"),
        Path::new("local-root/archive.bin"),
        Path::new("source-does-not-exist"),
        42,
        token,
    )
    .unwrap_err()
}

#[test]
fn bearer_credentials_reject_values_outside_rfc_6750_b64token_grammar() {
    for token in [
        "token with space",
        "token:colon",
        "token=padding=inside",
        "tokén",
    ] {
        assert_eq!(upload_error(token), "provider-api-bearer-token-invalid", "{token:?}");
        assert_eq!(
            delete_uploaded_object(CloudProvider::Icloud, "object-1", token).unwrap_err(),
            "provider-api-bearer-token-invalid",
            "{token:?}"
        );
    }
}

#[test]
fn bearer_credentials_keep_rfc_6750_token_characters_and_trailing_padding_admissible() {
    for token in ["mF_9.B5f-4.1JqM", "abc+/~._-=="] {
        assert_eq!(upload_error(token), "provider-api-icloud-unsupported", "{token:?}");
        assert_eq!(
            delete_uploaded_object(CloudProvider::Icloud, "object-1", token).unwrap_err(),
            "provider-api-icloud-unsupported",
            "{token:?}"
        );
    }
}
