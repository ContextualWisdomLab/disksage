#![allow(dead_code, unused_imports)]

//! Credential-free coverage for the OAuth callback query parser.
//!
//! The parser is intentionally private. These tests include the production module so percent
//! decoding, CSRF comparison, duplicate-field rejection, denial handling, and authorization-code
//! bounds are exercised without opening a listener, contacting a provider, touching the keyring,
//! or publishing durable OAuth metadata.

include!("../src/provider_oauth.rs");

mod cloud {
    pub use disksage_lib::cloud::*;
}

fn callback_error(target: &str, expected_state: &str) -> String {
    match callback_code(target, expected_state) {
        Ok(_) => panic!("invalid callback unexpectedly gained authorization-code authority"),
        Err(error) => error,
    }
}

#[test]
fn percent_decoding_and_constant_time_state_comparison_cover_valid_boundaries() {
    assert_eq!(
        percent_decode("plain+space%2fslash%7E").unwrap(),
        "plain space/slash~"
    );
    assert_eq!(percent_decode("%E2%82%AC").unwrap(), "€");

    for invalid in ["%", "%A", "%GG", "%FF"] {
        assert_eq!(
            percent_decode(invalid).unwrap_err(),
            "oauth-callback-query-invalid",
            "malformed or non-UTF-8 percent encoding must fail closed"
        );
    }

    assert!(constant_time_eq("same-state", "same-state"));
    assert!(!constant_time_eq("same-state", "same-State"));
    assert!(!constant_time_eq("short", "longer"));

    assert_eq!(
        callback_code(
            "/?code=abc%2D123&state=state+value&ignored=bounded",
            "state value"
        )
        .unwrap(),
        "abc-123",
        "valid form encoding must be decoded before state and code admission"
    );
}

#[test]
fn malformed_duplicate_denied_and_unbounded_callbacks_fail_closed() {
    assert_eq!(
        callback_error("/callback?code=one&state=state", "state"),
        "oauth-callback-path-invalid"
    );
    assert_eq!(
        callback_error("/?broken&state=state", "state"),
        "oauth-callback-query-invalid"
    );
    assert_eq!(
        callback_error("/?code=one&code=two&state=state", "state"),
        "oauth-callback-query-duplicate"
    );
    assert_eq!(
        callback_error("/?code=one&state=state&state=state", "state"),
        "oauth-callback-query-duplicate"
    );
    assert_eq!(
        callback_error("/?code=one", "state"),
        "oauth-callback-state-missing"
    );
    assert_eq!(
        callback_error("/?code=one&state=wrong", "state"),
        "oauth-callback-state-mismatch"
    );
    assert_eq!(
        callback_error("/?error=access_denied&state=state", "state"),
        "oauth-authorization-denied"
    );
    assert_eq!(
        callback_error("/?state=state", "state"),
        "oauth-callback-code-missing"
    );

    for target in ["/?code=&state=state", "/?code=%0A&state=state"] {
        assert_eq!(
            callback_error(target, "state"),
            "oauth-callback-code-invalid"
        );
    }

    let oversized = "a".repeat(MAX_TOKEN_BYTES + 1);
    assert_eq!(
        callback_error(&format!("/?code={oversized}&state=state"), "state"),
        "oauth-callback-code-invalid"
    );
}
