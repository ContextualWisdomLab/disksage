#!/usr/bin/env python3
"""Restore and verify pull request 125's public-documentation contract."""

from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUST_SOURCE = ROOT / "src-tauri/src/cloud_transfer.rs"
TYPESCRIPT_SOURCE = ROOT / "src/lib/api.ts"
CONTRACT_TEST = ROOT / "src-tauri/tests/cloud_transfer_coverage_contract.rs"


def replace_once(path: Path, old: str, new: str) -> None:
    """Replace exactly one expected source anchor or fail before writing."""
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one anchor in {path}, found {count}: {old[:96]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def restore_contract_test() -> None:
    """Restore the regression test that prevents documentation-gate weakening."""
    text = CONTRACT_TEST.read_text(encoding="utf-8")
    marker = "fn cloud_copy_approval_public_surfaces_remain_documented()"
    if marker in text:
        return
    addition = r'''

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
        "/** Builds the exact confirmation phrase shown to and entered by the human reviewer. */\nexport const cloudCopyApprovalPhrase",
    ] {
        assert!(
            typescript_source.contains(documented_declaration),
            "missing required TypeScript public documentation contract: {documented_declaration}"
        );
    }
}
'''
    CONTRACT_TEST.write_text(text.rstrip() + addition, encoding="utf-8")


def apply_documentation() -> None:
    """Add beginner-readable documentation to the newly introduced public surfaces."""
    replace_once(
        RUST_SOURCE,
        """pub const LEGACY_RECEIPT_VERSION: u32 = 2;
pub const PRE_APPROVAL_RECEIPT_VERSION: u32 = 3;
pub const RECEIPT_VERSION: u32 = 4;
pub const CLOUD_COPY_APPROVAL_VERSION: u32 = 1;
pub const MAX_CLOUD_COPY_APPROVAL_AGE_MS: u64 = 15 * 60 * 1000;
""",
        """/// Legacy receipt schema version retained for backward-compatible reads.
pub const LEGACY_RECEIPT_VERSION: u32 = 2;
/// Receipt schema version used before exact action approvals were embedded.
pub const PRE_APPROVAL_RECEIPT_VERSION: u32 = 3;
/// Current immutable cloud-copy receipt schema version.
pub const RECEIPT_VERSION: u32 = 4;
/// Schema version for one exact human cloud-copy approval.
pub const CLOUD_COPY_APPROVAL_VERSION: u32 = 1;
/// Maximum age accepted for an exact cloud-copy approval.
pub const MAX_CLOUD_COPY_APPROVAL_AGE_MS: u64 = 15 * 60 * 1000;
""",
    )
    replace_once(
        RUST_SOURCE,
        """#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = \"kebab-case\")]
pub enum CloudCopyApprovalAction {
    CopyOnly,
    AdoptExistingCopy,
}

impl CloudCopyApprovalAction {
    pub fn as_str(self) -> &'static str {
""",
        """/// Identifies the exact cloud-copy action authorized by a human reviewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = \"kebab-case\")]
pub enum CloudCopyApprovalAction {
    /// Authorize creating a new provider copy while retaining the local source.
    CopyOnly,
    /// Authorize adopting an already-existing destination after digest verification.
    AdoptExistingCopy,
}

impl CloudCopyApprovalAction {
    /// Return the stable kebab-case value stored in receipts and confirmation phrases.
    pub fn as_str(self) -> &'static str {
""",
    )
    replace_once(
        RUST_SOURCE,
        """pub struct CloudCopyApproval {
    pub version: u32,
    pub approval_id: String,
    pub action: CloudCopyApprovalAction,
    pub candidate_fingerprint: String,
    pub review_fingerprint: String,
    pub provider: CloudProvider,
    pub destination_account_scope: CloudAccountScope,
    pub cloud_root_id: String,
    pub approved_at_ms: u64,
    pub approved_by: String,
    pub rationale: String,
    pub exact_confirmation_phrase: String,
}
""",
        """pub struct CloudCopyApproval {
    /// Version of the approval record schema.
    pub version: u32,
    /// Integrity digest binding every field in this approval.
    pub approval_id: String,
    /// Exact copy or adoption action the reviewer authorized.
    pub action: CloudCopyApprovalAction,
    /// Metadata fingerprint of the candidate shown to the reviewer.
    pub candidate_fingerprint: String,
    /// Review fingerprint binding source, destination, scope, and displayed evidence.
    pub review_fingerprint: String,
    /// Cloud provider that will receive or already contains the destination object.
    pub provider: CloudProvider,
    /// Account boundary in which the destination is located.
    pub destination_account_scope: CloudAccountScope,
    /// Stable identifier of the reviewed cloud root.
    pub cloud_root_id: String,
    /// Millisecond Unix timestamp at which the reviewer approved the action.
    pub approved_at_ms: u64,
    /// Human-attributed reviewer identifier, such as `human:operator-id`.
    pub approved_by: String,
    /// Reviewer-authored explanation for approving this exact action.
    pub rationale: String,
    /// Exact candidate-specific phrase entered by the reviewer.
    pub exact_confirmation_phrase: String,
}
""",
    )
    replace_once(
        RUST_SOURCE,
        "pub fn cloud_copy_approval_phrase(\n",
        """/// Build the exact phrase a human must enter for one candidate and action.
///
/// The phrase includes the action and current review fingerprint, preventing a generic approval
/// from being replayed for a different source, destination, account scope, or operation.
pub fn cloud_copy_approval_phrase(
""",
    )
    replace_once(
        RUST_SOURCE,
        "pub fn create_cloud_copy_approval(\n",
        """/// Create an integrity-bound approval after validating the candidate, destination, actor, and phrase.
///
/// This constructor fails closed when the candidate fingerprint is stale, the cloud root does not
/// match the candidate, the reviewer attribution is incomplete, or the exact phrase differs.
pub fn create_cloud_copy_approval(
""",
    )
    replace_once(
        RUST_SOURCE,
        """#[cfg(all(test, not(coverage)))]
pub fn prepare_cloud_copy(
""",
        """/// Test-only compatibility helper that creates a valid exact approval before preparing a copy.
#[cfg(all(test, not(coverage)))]
pub fn prepare_cloud_copy(
""",
    )
    replace_once(
        RUST_SOURCE,
        """#[cfg(all(test, not(coverage)))]
pub fn prepare_cloud_copy_with_review(
""",
        """/// Test-only compatibility helper that combines metadata review and exact copy approval fixtures.
#[cfg(all(test, not(coverage)))]
pub fn prepare_cloud_copy_with_review(
""",
    )
    replace_once(
        RUST_SOURCE,
        """#[cfg(all(test, not(coverage)))]
pub fn adopt_existing_cloud_copy(
""",
        """/// Test-only compatibility helper that approves and verifies adoption of an existing copy.
#[cfg(all(test, not(coverage)))]
pub fn adopt_existing_cloud_copy(
""",
    )
    replace_once(
        TYPESCRIPT_SOURCE,
        """export type CloudCopyVerificationMethod = \"copied-by-disk-sage\" | \"adopted-existing\";
export type CloudCopyApprovalAction = \"copy-only\" | \"adopt-existing-copy\";

export interface CloudCopyApproval {
""",
        """export type CloudCopyVerificationMethod = \"copied-by-disk-sage\" | \"adopted-existing\";
/** Identifies the exact cloud-copy action authorized by a human reviewer. */
export type CloudCopyApprovalAction = \"copy-only\" | \"adopt-existing-copy\";

/** Records who approved one exact candidate, destination, and action, and when. */
export interface CloudCopyApproval {
""",
    )
    replace_once(
        TYPESCRIPT_SOURCE,
        "export const cloudCopyApprovalPhrase = (\n",
        """/** Builds the exact confirmation phrase shown to and entered by the human reviewer. */
export const cloudCopyApprovalPhrase = (
""",
    )


def verify_repair() -> None:
    """Fail unless documentation and the non-weakened regression contract are present."""
    rust_source = RUST_SOURCE.read_text(encoding="utf-8")
    typescript_source = TYPESCRIPT_SOURCE.read_text(encoding="utf-8")
    contract_source = CONTRACT_TEST.read_text(encoding="utf-8")
    required_markers = (
        "/// Receipt schema version used before exact action approvals were embedded.\npub const PRE_APPROVAL_RECEIPT_VERSION",
        "/// Current immutable cloud-copy receipt schema version.\npub const RECEIPT_VERSION",
        "/// Schema version for one exact human cloud-copy approval.\npub const CLOUD_COPY_APPROVAL_VERSION",
        "/// Maximum age accepted for an exact cloud-copy approval.\npub const MAX_CLOUD_COPY_APPROVAL_AGE_MS",
        "/// Identifies the exact cloud-copy action authorized by a human reviewer.\n#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]",
        "/// Return the stable kebab-case value stored in receipts and confirmation phrases.\n    pub fn as_str",
        "/// Build the exact phrase a human must enter for one candidate and action.",
        "/// Create an integrity-bound approval after validating the candidate, destination, actor, and phrase.",
    )
    missing = [marker for marker in required_markers if marker not in rust_source]
    for marker in (
        "/** Identifies the exact cloud-copy action authorized by a human reviewer. */\nexport type CloudCopyApprovalAction",
        "/** Records who approved one exact candidate, destination, and action, and when. */\nexport interface CloudCopyApproval",
        "/** Builds the exact confirmation phrase shown to and entered by the human reviewer. */\nexport const cloudCopyApprovalPhrase",
    ):
        if marker not in typescript_source:
            missing.append(marker)
    if "fn cloud_copy_approval_public_surfaces_remain_documented()" not in contract_source:
        missing.append("cloud_copy_approval_public_surfaces_remain_documented")
    if missing:
        raise SystemExit(f"documentation contract incomplete: {missing}")


def main() -> None:
    """Restore the test, apply documentation, and verify the bounded repair."""
    restore_contract_test()
    apply_documentation()
    verify_repair()


if __name__ == "__main__":
    main()
