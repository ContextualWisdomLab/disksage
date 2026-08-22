use disksage_lib::cloud::CloudProvider;
use disksage_lib::cloud_transfer::{
    receipt_blockers, CloudCopyApprovalAction, CloudCopyReceipt, LEGACY_RECEIPT_VERSION,
    RECEIPT_VERSION,
};

fn receipt() -> CloudCopyReceipt {
    CloudCopyReceipt {
        version: RECEIPT_VERSION,
        receipt_id: "0".repeat(64),
        candidate_fingerprint: "a".repeat(64),
        provider: CloudProvider::Onedrive,
        source: "/tmp/disksage-source.bin".into(),
        destination: "/tmp/disksage-destination.bin".into(),
        bytes: 12,
        blake3: "b".repeat(64),
        sha256: "c".repeat(64),
        quick_xor_base64: "quick-xor".into(),
        source_modified_ms: 1,
        copied_at_ms: 2,
        copy_verified: true,
        provider_sync_confirmed: false,
        lineage_fingerprint: None,
        lineage: None,
    }
}

fn assert_has(blockers: &[String], expected: &str) {
    assert!(
        blockers.iter().any(|blocker| blocker == expected),
        "expected blocker {expected:?}, got {blockers:?}"
    );
}

#[test]
fn copy_approval_actions_keep_stable_receipt_wire_labels() {
    assert_eq!(CloudCopyApprovalAction::CopyOnly.as_str(), "copy-only");
    assert_eq!(
        CloudCopyApprovalAction::AdoptExistingCopy.as_str(),
        "adopt-existing-copy"
    );
}

#[test]
fn receipt_blockers_reject_unsupported_and_missing_lineage_shapes() {
    let current = receipt();
    let blockers = receipt_blockers(&current);
    assert_has(&blockers, "receipt-lineage-missing");
    assert_has(&blockers, "receipt-integrity-mismatch");

    let mut unsupported = current;
    unsupported.version = u32::MAX;
    let blockers = receipt_blockers(&unsupported);
    assert_has(&blockers, "receipt-version-unsupported");
    assert_has(&blockers, "receipt-integrity-mismatch");
}

#[test]
fn legacy_receipts_reject_lineage_fields_that_did_not_exist_in_that_schema() {
    let mut legacy = receipt();
    legacy.version = LEGACY_RECEIPT_VERSION;
    legacy.lineage_fingerprint = Some("d".repeat(64));

    let blockers = receipt_blockers(&legacy);
    assert_has(&blockers, "legacy-receipt-lineage-unexpected");
    assert_has(&blockers, "receipt-integrity-mismatch");
}

#[test]
fn receipt_blockers_reject_unverified_consumed_and_unsafe_paths() {
    let mut unverified = receipt();
    unverified.copy_verified = false;
    assert_has(&receipt_blockers(&unverified), "copy-not-verified");

    let mut consumed = receipt();
    consumed.provider_sync_confirmed = true;
    assert_has(&receipt_blockers(&consumed), "receipt-already-consumed");

    let mut relative_source = receipt();
    relative_source.source = "relative/source.bin".into();
    assert_has(
        &receipt_blockers(&relative_source),
        "receipt-source-path-not-safe-absolute",
    );

    let mut traversing_source = receipt();
    traversing_source.source = "/tmp/disksage/../source.bin".into();
    assert_has(
        &receipt_blockers(&traversing_source),
        "receipt-source-path-not-safe-absolute",
    );

    let mut relative_destination = receipt();
    relative_destination.destination = "relative/destination.bin".into();
    assert_has(
        &receipt_blockers(&relative_destination),
        "receipt-destination-path-not-safe-absolute",
    );

    let mut traversing_destination = receipt();
    traversing_destination.destination = "/tmp/disksage/../destination.bin".into();
    assert_has(
        &receipt_blockers(&traversing_destination),
        "receipt-destination-path-not-safe-absolute",
    );

    let mut same_path = receipt();
    same_path.destination = same_path.source.clone();
    assert_has(
        &receipt_blockers(&same_path),
        "receipt-source-equals-destination",
    );
}
