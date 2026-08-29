use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_client_runtime::assess_provider_client_runtime;

#[test]
fn onedrive_helper_does_not_force_a_quit_when_the_primary_app_is_stopped() {
    // The broad copy-prerequisite observation intentionally accepts the sync helper.
    let helper_only = b"Finder\nOneDrive Sync Service\n";
    let broad = assess_provider_client_runtime(CloudProvider::Onedrive, Some(helper_only), 42);
    assert_eq!(broad.runtime_observed, Some(true));

    // The destructive local-eviction boundary must make its quit decision from the narrower
    // primary-process observation, so a lingering helper cannot turn an already-stopped app into
    // a failing AppleScript quit request.
    let recovery_source = include_str!("../src/provider_recovery.rs");
    let unpin = recovery_source
        .split_once("pub(crate) fn unpin_onedrive_local_copy")
        .expect("OneDrive unpin function")
        .1
        .split_once("fn runtime_observation")
        .expect("OneDrive unpin function boundary")
        .0;
    assert!(unpin.contains("let primary_runtime_observed ="));
    assert!(unpin.contains("collect_provider_primary_runtime"));
    assert!(unpin.contains("if primary_runtime_observed {\n        request_quit(\"OneDrive\")?;\n    }"));

    // A fully closed primary app is already in the state required by OneDrive `/unpin`.
    // Requiring the broad helper-aware observation here would incorrectly reject that safe state.
    assert!(!unpin.contains("require_runtime_observation(CloudProvider::Onedrive, 0)"));

    let runtime_source = include_str!("../src/provider_client_runtime.rs");
    let primary_observer = runtime_source
        .split_once("pub(crate) fn collect_provider_primary_runtime")
        .expect("primary provider runtime observer")
        .1
        .split_once("pub fn require_provider_client_runtime")
        .expect("primary provider runtime observer boundary")
        .0;
    assert!(primary_observer.contains("CloudProvider::Onedrive => \"OneDrive\""));
    assert!(!primary_observer.contains("OneDrive Sync Service"));
}

#[test]
fn failed_shutdown_requests_are_judged_by_primary_process_evidence() {
    let recovery_source = include_str!("../src/provider_recovery.rs");
    let request_quit = recovery_source
        .split_once("fn request_quit")
        .expect("quit request helper")
        .1
        .split_once("fn request_graceful_term")
        .expect("quit request helper boundary")
        .0;
    assert!(request_quit.contains("require_primary_runtime_observation"));
    assert!(!request_quit.contains("require_runtime_observation(provider, 0)"));

    let graceful_term = recovery_source
        .split_once("fn request_graceful_term")
        .expect("graceful termination helper")
        .1
        .split_once("pub fn recover_provider_client")
        .expect("graceful termination helper boundary")
        .0;
    assert!(graceful_term.contains("require_primary_runtime_observation"));
    assert!(!graceful_term.contains("require_runtime_observation(provider, 0)"));

    let primary_requirement = recovery_source
        .split_once("fn require_primary_runtime_observation")
        .expect("primary runtime requirement")
        .1
        .split_once("fn request_quit")
        .expect("primary runtime requirement boundary")
        .0;
    assert!(primary_requirement.contains("collect_provider_primary_runtime"));
}
