//! Source contract for cloud-transfer coverage instrumentation.
//!
//! The approval fixture is required by ordinary Rust tests even when the central coverage runner
//! adds `--cfg coverage`. This regression prevents a future refactor from excluding the helper and
//! breaking the receipt-lineage tests before coverage can be measured.

/// Verifies that the deterministic approval fixture remains compiled for every test build.
#[test]
fn approval_fixture_remains_available_during_coverage_builds() {
    let source = include_str!("../src/cloud_transfer.rs");

    assert!(
        source.contains("#[cfg(test)]\nfn test_copy_approval("),
        "test_copy_approval must remain available when cfg(coverage) is active"
    );
    assert!(
        !source.contains("#[cfg(all(test, not(coverage)))]\nfn test_copy_approval("),
        "test_copy_approval must not be excluded from coverage-mode test compilation"
    );
}

/// Verifies that exact-copy approval public APIs keep beginner-readable documentation.
#[test]
fn cloud_copy_approval_public_surfaces_remain_documented() {
    let rust_source = include_str!("../src/cloud_transfer.rs");
    let typescript_source = include_str!("../../src/lib/api.ts");

    for documented_declaration in [
        "/// Receipt schema version used before exact action approvals were embedded.\npub const PRE_APPROVAL_RECEIPT_VERSION",
        "/// Current immutable cloud-copy receipt schema version.\npub const RECEIPT_VERSION",
        "/// Schema version for one exact human cloud-copy approval.\npub const CLOUD_COPY_APPROVAL_VERSION",
        "/// Maximum age accepted for an exact cloud-copy approval.\npub const MAX_CLOUD_COPY_APPROVAL_AGE_MS",
        "/// Identifies the exact cloud-copy action authorized by a human reviewer.\n#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = \"kebab-case\")]\npub enum CloudCopyApprovalAction",
        "/// Return the stable kebab-case value stored in receipts and confirmation phrases.\n    pub fn as_str",
        "/// Build the exact phrase a human must enter for one candidate and action.\n///\n/// The phrase includes the action and current review fingerprint, preventing a generic approval\n/// from being replayed for a different source, destination, account scope, or operation.\npub fn cloud_copy_approval_phrase",
        "/// Create an integrity-bound approval after validating the candidate, destination, actor, and phrase.\n///\n/// This constructor fails closed when the candidate fingerprint is stale, the cloud root does not\n/// match the candidate, the reviewer attribution is incomplete, or the exact phrase differs.\npub fn create_cloud_copy_approval",
    ] {
        assert!(
            rust_source.contains(documented_declaration),
            "missing required Rust public documentation contract: {documented_declaration}"
        );
    }

    for documented_declaration in [
        "/** Identifies the exact cloud-copy action authorized by a human reviewer. */\nexport type CloudCopyApprovalAction",
        "/** Records who approved one exact candidate, destination, and action, and when. */\nexport interface CloudCopyApproval",
        "/** Returns the exact backend-authored phrase only for the matching candidate action. */\nexport const cloudCopyApprovalPhrase",
    ] {
        assert!(
            typescript_source.contains(documented_declaration),
            "missing required TypeScript public documentation contract: {documented_declaration}"
        );
    }
}

/// Verifies that plans expose backend-authored phrases and the frontend never reconstructs them.
#[test]
fn cloud_plan_exports_backend_authored_approval_phrase() {
    let view_source = include_str!("../src/cloud_plan_view.rs");
    let command_source = include_str!("../src/commands.rs");
    let api_source = include_str!("../../src/lib/api.ts");
    let ui_source = include_str!("../../src/lib/CloudArchive.svelte");

    for marker in [
        "pub struct CloudPlanCandidateView",
        "pub copy_approval_action: Option<CloudCopyApprovalAction>",
        "pub exact_copy_approval_phrase: Option<String>",
        "pub copy_approval_max_age_ms: u64",
        "cloud_copy_approval_phrase(&candidate, action)",
    ] {
        assert!(view_source.contains(marker), "missing backend plan-view marker: {marker}");
    }
    assert!(
        command_source.contains("Result<cloud_plan_view::CloudPlanReportView, String>"),
        "Tauri plan command must return the typed backend-authored view"
    );
    for marker in [
        "copy_approval_action?: CloudCopyApprovalAction | null",
        "exact_copy_approval_phrase?: string | null",
        "copy_approval_max_age_ms?: number",
        "/** Returns the exact backend-authored phrase only for the matching candidate action. */",
    ] {
        assert!(api_source.contains(marker), "missing frontend plan contract: {marker}");
    }
    assert!(
        !api_source.contains("`DiskSage cloud ${action} ${candidate.review_fingerprint} 승인`"),
        "frontend must not reconstruct the authorization phrase"
    );
    for marker in [
        "{@const copyApprovalPhrase = api.cloudCopyApprovalPhrase(candidate, \"copy-only\")}",
        "{@const adoptApprovalPhrase = api.cloudCopyApprovalPhrase(candidate, \"adopt-existing-copy\")}",
    ] {
        assert!(ui_source.contains(marker), "approval phrase must be evaluated once: {marker}");
    }
}
