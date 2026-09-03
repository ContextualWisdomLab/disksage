#![allow(dead_code, unused_imports)]

//! Credential-free coverage for the OAuth token-response admission boundary.
//!
//! The production parser stays private because provider token documents are not a product API.
//! These regressions include the production module so malformed and boundary responses exercise
//! the shipped parser without contacting a provider, opening a browser, or touching the keyring.

include!("../src/provider_oauth.rs");

mod cloud {
    pub use disksage_lib::cloud::*;
}

fn parse_google(json: &str, refresh_required: bool) -> Result<OAuthGrant, String> {
    parse_token_document(
        CloudProvider::GoogleDrive,
        GOOGLE_READ_SCOPE,
        json,
        refresh_required,
    )
}

fn parse_error(result: Result<OAuthGrant, String>) -> String {
    match result {
        Ok(_) => panic!("token document unexpectedly gained OAuth grant authority"),
        Err(error) => error,
    }
}

#[test]
fn malformed_or_rejected_token_documents_fail_closed_before_secret_use() {
    assert_eq!(
        parse_error(parse_google("not-json", false)),
        "oauth-token-response-invalid"
    );
    assert_eq!(
        parse_error(parse_google(r#"{"error":"invalid_grant"}"#, false)),
        "oauth-token-endpoint-rejected"
    );

    for json in [
        r#"{"token_type":"Bearer"}"#,
        r#"{"access_token":"","token_type":"Bearer"}"#,
        r#"{"access_token":"line\nfeed","token_type":"Bearer"}"#,
    ] {
        assert_eq!(
            parse_error(parse_google(json, false)),
            "oauth-access-token-invalid",
            "missing, empty, or control-bearing access tokens must fail closed"
        );
    }

    let oversized_access = "a".repeat(MAX_TOKEN_BYTES + 1);
    let json = serde_json::json!({
        "access_token": oversized_access,
        "token_type": "Bearer"
    })
    .to_string();
    assert_eq!(
        parse_error(parse_google(&json, false)),
        "oauth-access-token-invalid"
    );
}

#[test]
fn bearer_type_expiry_and_resource_scope_are_bounded() {
    for json in [
        r#"{"access_token":"access"}"#,
        r#"{"access_token":"access","token_type":"MAC"}"#,
    ] {
        assert_eq!(
            parse_error(parse_google(json, false)),
            "oauth-token-type-invalid"
        );
    }

    for expires_in in [0_u64, 86_401] {
        let json = serde_json::json!({
            "access_token": "access",
            "token_type": "Bearer",
            "expires_in": expires_in
        })
        .to_string();
        assert_eq!(
            parse_error(parse_google(&json, false)),
            "oauth-token-expiry-invalid"
        );
    }

    let legal_max_expiry = parse_google(
        r#"{"access_token":"access","token_type":"bEaReR","expires_in":86400}"#,
        false,
    )
    .expect("the documented maximum token lifetime and case-insensitive bearer type are valid");
    assert_eq!(legal_max_expiry.access_token.as_str(), "access");

    assert_eq!(
        parse_error(parse_google(
            r#"{"access_token":"access","token_type":"Bearer","scope":"https://www.googleapis.com/auth/drive.file"}"#,
            false,
        )),
        "oauth-required-scope-missing"
    );

    let with_extra_scope = parse_google(
        r#"{"access_token":"access","token_type":"Bearer","scope":"openid https://www.googleapis.com/auth/drive.metadata.readonly profile"}"#,
        false,
    )
    .expect("the required Google Drive resource scope may appear among additional grants");
    assert_eq!(with_extra_scope.access_token.as_str(), "access");

    let omitted_scope = parse_google(
        r#"{"access_token":"access","token_type":"Bearer"}"#,
        false,
    )
    .expect("providers may omit the optional scope echo when the issued access scope is unchanged");
    assert_eq!(omitted_scope.access_token.as_str(), "access");
}

#[test]
fn microsoft_scope_echo_represents_access_scope_while_refresh_token_proves_offline_grant() {
    let grant = parse_token_document(
        CloudProvider::Onedrive,
        ONEDRIVE_READ_SCOPE,
        r#"{"access_token":"access","refresh_token":"refresh","token_type":"Bearer","scope":"Files.Read"}"#,
        true,
    )
    .expect("Microsoft access-token scope may omit offline_access while returning a refresh token");

    assert_eq!(grant.access_token.as_str(), "access");
    assert_eq!(grant.refresh_token.unwrap().as_str(), "refresh");

    assert_eq!(
        parse_error(parse_token_document(
            CloudProvider::Onedrive,
            ONEDRIVE_READ_SCOPE,
            r#"{"access_token":"access","refresh_token":"refresh","token_type":"Bearer","scope":"Files.ReadWrite"}"#,
            true,
        )),
        "oauth-required-scope-missing",
        "a token for a different Microsoft Graph resource scope must not satisfy the requested scope"
    );
}

#[test]
fn refresh_token_requirement_and_value_bounds_fail_closed() {
    assert_eq!(
        parse_error(parse_google(
            r#"{"access_token":"access","token_type":"Bearer"}"#,
            true,
        )),
        "oauth-refresh-token-missing"
    );

    for refresh_token in ["", "refresh\nvalue"] {
        let json = serde_json::json!({
            "access_token": "access",
            "refresh_token": refresh_token,
            "token_type": "Bearer"
        })
        .to_string();
        assert_eq!(
            parse_error(parse_google(&json, true)),
            "oauth-refresh-token-invalid"
        );
    }

    let oversized_refresh = "r".repeat(MAX_TOKEN_BYTES + 1);
    let json = serde_json::json!({
        "access_token": "access",
        "refresh_token": oversized_refresh,
        "token_type": "Bearer"
    })
    .to_string();
    assert_eq!(
        parse_error(parse_google(&json, true)),
        "oauth-refresh-token-invalid"
    );

    let valid = parse_google(
        r#"{"access_token":"access","refresh_token":"refresh","token_type":"Bearer"}"#,
        true,
    )
    .expect("a bounded refresh token is required and accepted on initial authorization");
    assert_eq!(valid.access_token.as_str(), "access");
    assert_eq!(valid.refresh_token.unwrap().as_str(), "refresh");
}
