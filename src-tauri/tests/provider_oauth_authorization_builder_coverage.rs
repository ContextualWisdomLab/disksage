#![allow(dead_code, unused_imports)]

//! Credential-free coverage for the private OAuth authorization URL builder.
//!
//! These tests include the production module so the validation and percent-encoding branches can
//! be exercised without widening the shipped API. They do not bind a listener, open a browser,
//! contact a provider, access the credential store, or mutate cloud state.

include!("../src/provider_oauth.rs");

mod cloud {
    pub use disksage_lib::cloud::*;
}

const MICROSOFT_CLIENT_ID: &str = "12345678-1234-4abc-8def-1234567890ab";
const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";
const PKCE_43: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const STATE_43: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn build_error(
    provider: CloudProvider,
    client_id: &str,
    redirect_uri: &str,
    challenge: &str,
    state: &str,
) -> String {
    match build_authorization_url(provider, client_id, redirect_uri, challenge, state) {
        Ok(_) => panic!("invalid authorization inputs unexpectedly produced an OAuth URL"),
        Err(error) => error,
    }
}

#[test]
fn redirect_port_and_pkce_material_fail_closed_before_authorization() {
    for redirect_uri in [
        "https://localhost:49152",
        "http://127.0.0.1:49152",
        "http://localhost:",
        "http://localhost:abc",
        "http://localhost:0",
        "http://localhost:65536",
    ] {
        assert_eq!(
            build_error(
                CloudProvider::Onedrive,
                MICROSOFT_CLIENT_ID,
                redirect_uri,
                PKCE_43,
                STATE_43,
            ),
            "oauth-redirect-uri-invalid",
            "OneDrive must admit only a non-zero u16 localhost loopback port"
        );
    }

    assert_eq!(
        build_error(
            CloudProvider::GoogleDrive,
            GOOGLE_CLIENT_ID,
            "http://localhost:49152",
            PKCE_43,
            STATE_43,
        ),
        "oauth-redirect-uri-invalid"
    );

    for (challenge, state) in [
        ("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", STATE_43),
        (PKCE_43, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
    ] {
        assert_eq!(
            build_error(
                CloudProvider::GoogleDrive,
                GOOGLE_CLIENT_ID,
                "http://127.0.0.1:49152",
                challenge,
                state,
            ),
            "oauth-pkce-material-invalid",
            "PKCE challenge and CSRF state must both be exactly 43 URL-safe bytes"
        );
    }
}

#[test]
fn provider_specific_authorization_queries_are_exact_and_secret_free() {
    let onedrive = build_authorization_url(
        CloudProvider::Onedrive,
        MICROSOFT_CLIENT_ID,
        "http://localhost:49152",
        PKCE_43,
        STATE_43,
    )
    .expect("valid OneDrive authorization inputs");
    assert_eq!(
        onedrive,
        concat!(
            "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?",
            "client_id=12345678-1234-4abc-8def-1234567890ab",
            "&redirect_uri=http%3A%2F%2Flocalhost%3A49152",
            "&response_type=code",
            "&scope=Files.Read%20offline_access",
            "&state=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "&code_challenge=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "&code_challenge_method=S256",
            "&response_mode=query",
            "&prompt=select_account"
        )
    );

    let google = build_authorization_url(
        CloudProvider::GoogleDrive,
        GOOGLE_CLIENT_ID,
        "http://127.0.0.1:49152",
        PKCE_43,
        STATE_43,
    )
    .expect("valid Google Drive authorization inputs");
    assert_eq!(
        google,
        concat!(
            "https://accounts.google.com/o/oauth2/v2/auth?",
            "client_id=1234567890-abcxyz.apps.googleusercontent.com",
            "&redirect_uri=http%3A%2F%2F127.0.0.1%3A49152",
            "&response_type=code",
            "&scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.metadata.readonly",
            "&state=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "&code_challenge=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "&code_challenge_method=S256",
            "&access_type=offline",
            "&prompt=consent",
            "&include_granted_scopes=true"
        )
    );

    for url in [&onedrive, &google] {
        assert!(!url.contains("code_verifier"));
        assert!(!url.contains("access_token"));
        assert!(!url.contains("refresh_token"));
    }
}

#[test]
fn percent_encoding_preserves_only_the_rfc3986_unreserved_set() {
    assert_eq!(
        percent_encode("AZaz09-._~ :/?&=+%"),
        "AZaz09-._~%20%3A%2F%3F%26%3D%2B%25"
    );
    assert_eq!(
        query_url("https://example.invalid/path", &[("a b", "x/y"), ("z", "~")]),
        "https://example.invalid/path?a%20b=x%2Fy&z=~"
    );
}
