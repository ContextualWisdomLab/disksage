//! Regression contract for OneDrive local-space recovery under provider-wide backlog.

#[test]
fn onedrive_native_eviction_uses_foundation_after_exact_live_revalidation() {
    let source = include_str!("../src/cloud_local_eviction.rs");
    let function = source
        .split_once("fn request_native_icloud_eviction")
        .expect("native eviction function")
        .1
        .split_once("fn observe_post_eviction")
        .expect("native eviction function boundary")
        .0;

    assert!(function.contains("evictUbiquitousItemAtURL_error"));
    assert!(function.contains("native-file-provider-item-identity-unconfirmed"));
    assert!(!function.contains("unpin_onedrive_local_copy"));
    assert!(!function.contains("inspect_new_copy_admission"));
    assert!(!function.contains("require_new_copy_admission"));
}

#[test]
fn onedrive_unpin_has_a_bounded_graceful_term_fallback() {
    let source = include_str!("../src/provider_recovery.rs");
    let function = source
        .split_once("pub(crate) fn unpin_onedrive_local_copy")
        .expect("OneDrive unpin function")
        .1
        .split_once("pub fn recover_provider_client")
        .expect("OneDrive unpin function boundary")
        .0;

    assert!(function.contains("request_quit(\"OneDrive\")"));
    assert!(function.contains("request_graceful_term(\"OneDrive\")"));
    assert!(function.contains("provider-recovery-quit-timeout"));
    assert!(function.contains("require_primary_runtime_observation"));
    assert!(!function.contains("\"/getpin\""));
    assert!(function.contains("\"/unpin\""));

    let graceful_term = source
        .split_once("fn request_graceful_term")
        .expect("graceful termination helper")
        .1
        .split_once("pub fn recover_provider_client")
        .expect("graceful termination helper boundary")
        .0;
    assert!(graceful_term.contains("\"-TERM\""));
    assert!(!graceful_term.contains("\"-KILL\""));

    let quit = source
        .split_once("fn request_quit")
        .expect("quit helper")
        .1
        .split_once("fn request_graceful_term")
        .expect("quit helper boundary")
        .0;
    assert!(quit.contains("require_primary_runtime_observation"));
    assert!(!quit.contains("require_runtime_observation"));
}
