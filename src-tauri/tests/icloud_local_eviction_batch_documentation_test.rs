//! Contract tests for the operator-visible iCloud batch safety documentation.
//!
//! These checks keep the operational claims, evidence boundary, standards mapping, and APA 7th
//! references reviewable alongside the Rust behavior they describe.

/// Operator guide compiled into the test binary so documentation claims are checked offline.
const OPERATOR_GUIDE: &str =
    include_str!("../../docs/development/icloud-local-eviction-batch.md");

/// Verifies that the operator guide maps fail-closed behavior to authoritative controls.
#[test]
fn operator_guide_maps_fail_closed_behavior_to_authoritative_controls() {
    for required_text in [
        "NIST SP 800-53 Release 5.2.0",
        "AC-3",
        "AU-9",
        "SI-10",
        "ISO/IEC 27040:2024",
        "fail-safe defaults",
        "complete mediation",
        "least privilege",
    ] {
        assert!(
            OPERATOR_GUIDE.contains(required_text),
            "operator guide must contain {required_text:?}"
        );
    }
}

/// Verifies that local-only evidence is clearly separated from shareable evidence.
#[test]
fn operator_guide_separates_local_only_and_shareable_evidence() {
    for required_text in [
        "Local-only evidence",
        "Shareable evidence",
        "never include source paths",
        "not a certification claim",
    ] {
        assert!(
            OPERATOR_GUIDE.contains(required_text),
            "operator guide must contain {required_text:?}"
        );
    }
}

/// Verifies that the operator guide contains complete APA 7th reference markers.
#[test]
fn operator_guide_contains_complete_apa_seventh_references() {
    for required_text in [
        "Joint Task Force. (2020).",
        "International Organization for Standardization & International Electrotechnical Commission. (2024).",
        "Saltzer, J. H., & Schroeder, M. D. (1975).",
        "https://doi.org/10.6028/NIST.SP.800-53r5",
        "https://doi.org/10.1109/PROC.1975.9939",
    ] {
        assert!(
            OPERATOR_GUIDE.contains(required_text),
            "operator guide must contain {required_text:?}"
        );
    }
}
