//! Source-level contract for the read-only Podman desktop integration.
//!
//! These tests complement behavior tests by making the product boundary reviewable: the desktop
//! adapter must remain registered, typed, path-redacted, fully documented, and free of destructive
//! Podman command surfaces.

const RUST_ADAPTER: &str = include_str!("../src/podman_desktop.rs");
const RUST_LIBRARY: &str = include_str!("../src/lib.rs");
const TYPESCRIPT_API: &str = include_str!("../../src/lib/podmanApi.ts");
const TYPESCRIPT_VIEW_MODEL: &str = include_str!("../../src/lib/podmanEvidence.ts");
const SVELTE_VIEW: &str = include_str!("../../src/lib/PodmanReclaimEvidence.svelte");
const CLEANUP_VIEW: &str = include_str!("../../src/lib/Cleanup.svelte");
const ADR: &str = include_str!("../../docs/adr/0001-podman-desktop-evidence-boundary.md");

/// Verifies that the desktop command is a narrow adapter around the existing Rust evidence engine.
#[test]
fn desktop_command_is_registered_and_argv_bound() {
    for marker in [
        "pub fn collect_podman_reclaim_plan_with",
        "pub async fn podman_reclaim_plan",
        "DEFAULT_PODMAN_MACHINE",
        "DEFAULT_PROBE_TIMEOUT",
        "crate::podman_reclaim::probe_podman_reclaim",
        "podman-reclaim-probe-join-failed",
    ] {
        assert!(RUST_ADAPTER.contains(marker), "missing Rust adapter marker: {marker}");
    }
    assert!(
        RUST_LIBRARY.contains("podman_desktop::podman_reclaim_plan"),
        "Tauri handler must register the read-only Podman evidence command"
    );
    assert!(
        !RUST_ADAPTER.contains("/bin/sh") && !RUST_ADAPTER.contains("cmd.exe"),
        "desktop adapter must not create a shell command"
    );
}

/// Verifies the versioned frontend API, redaction model, and Cleanup component composition.
#[test]
fn frontend_contract_is_typed_redacted_and_componentized() {
    for marker in [
        "export interface PodmanReclaimPlan",
        "export const podmanReclaimPlan",
        "invoke<PodmanReclaimPlan>(\"podman_reclaim_plan\"",
    ] {
        assert!(TYPESCRIPT_API.contains(marker), "missing typed API marker: {marker}");
    }
    for marker in [
        "export function safePodmanIssueCode",
        "export function podmanEvidenceMetrics",
        "export function podmanCandidateCategories",
        "evidence_class: \"configured\"",
        "evidence_class: \"logical_candidate\"",
        "evidence_class: \"physical_proof\"",
    ] {
        assert!(
            TYPESCRIPT_VIEW_MODEL.contains(marker),
            "missing redacted presentation marker: {marker}"
        );
    }
    assert!(
        CLEANUP_VIEW.contains("<PodmanReclaimEvidence />"),
        "Cleanup must compose the standalone Podman evidence component"
    );
    for forbidden in [
        "plan.machine.name",
        "plan.raw_image.path",
        "plan.store.graph_root",
    ] {
        assert!(
            !SVELTE_VIEW.contains(forbidden),
            "desktop view must not render local identifier: {forbidden}"
        );
    }
}

/// Verifies that no destructive Podman action is exposed by the new desktop command or API.
#[test]
fn desktop_surface_contains_no_destructive_podman_command() {
    for forbidden in [
        "podman_prune",
        "podman_remove",
        "podman_machine_stop",
        "podman_machine_start",
        "podman_machine_rm",
        "podman_trim",
        "prunePodman",
        "removePodman",
        "trimPodman",
    ] {
        assert!(
            !RUST_ADAPTER.contains(forbidden)
                && !TYPESCRIPT_API.contains(forbidden)
                && !TYPESCRIPT_VIEW_MODEL.contains(forbidden),
            "destructive Podman API marker is forbidden: {forbidden}"
        );
    }
}

/// Verifies that the architecture decision and standards references remain with the feature.
#[test]
fn adr_documents_evidence_classes_privacy_modularity_and_standards() {
    for marker in [
        "Configured",
        "Observed",
        "Logical candidate",
        "Physical proof",
        "local-only",
        "Modularity",
        "```mermaid",
        "ISO/IEC 27040:2024",
        "NIST SP 800-53, Release 5.2.0",
        "NIST SP 800-218",
        "References — APA 7th",
    ] {
        assert!(ADR.contains(marker), "missing ADR contract marker: {marker}");
    }
}

/// Verifies beginner-readable documentation on every introduced public TypeScript function.
#[test]
fn introduced_public_functions_retain_documentation() {
    for marker in [
        "/** Request a read-only Podman reclaim report from the Tauri backend. */\nexport const podmanReclaimPlan",
        "/** Return a stable localized label for a backend recommendation code. */\nexport function podmanActionLabel",
        "/** Reduce a potentially detailed backend issue string to a path-free stable code. */\nexport function safePodmanIssueCode",
        "/** Return sorted, de-duplicated, path-free issue codes for local presentation. */\nexport function podmanIssueCodes",
        "/** Return the redacted exact candidate-set fingerprint only when it is valid SHA-256 hex. */\nexport function podmanCandidateFingerprint",
        "/** Build the evidence rows shown by the desktop without exposing local names or paths. */\nexport function podmanEvidenceMetrics",
        "/** Keep image, stopped-container, and volume approvals as distinct candidate categories. */\nexport function podmanCandidateCategories",
    ] {
        assert!(
            TYPESCRIPT_API.contains(marker) || TYPESCRIPT_VIEW_MODEL.contains(marker),
            "missing public TypeScript documentation marker: {marker}"
        );
    }
}
