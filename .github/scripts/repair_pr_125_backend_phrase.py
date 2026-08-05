#!/usr/bin/env python3
"""Apply the test-first backend-authored approval-phrase repair for pull request 125."""

from __future__ import annotations

import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
COMMANDS = ROOT / "src-tauri/src/commands.rs"
LIB = ROOT / "src-tauri/src/lib.rs"
VIEW = ROOT / "src-tauri/src/cloud_plan_view.rs"
CONTRACT = ROOT / "src-tauri/tests/cloud_transfer_coverage_contract.rs"
API = ROOT / "src/lib/api.ts"
API_TEST = ROOT / "src/lib/api.test.ts"
UI = ROOT / "src/lib/CloudArchive.svelte"


def replace_once(path: Path, old: str, new: str) -> None:
    """Replace exactly one source anchor or fail before writing the file."""
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one anchor in {path}, found {count}: {old[:120]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def append_once(path: Path, marker: str, addition: str) -> None:
    """Append one regression block only when its stable marker is absent."""
    text = path.read_text(encoding="utf-8")
    if marker in text:
        raise SystemExit(f"marker already exists in {path}: {marker}")
    path.write_text(text.rstrip() + "\n\n" + addition.strip() + "\n", encoding="utf-8")


def add_tests() -> None:
    """Add failing contracts before any production implementation is created."""
    replace_once(
        API_TEST,
        '''describe("cloud copy approval phrase", () => {
  it("binds the exact review fingerprint and requested action", () => {
    const candidate = { review_fingerprint: "a".repeat(64) };
    expect(api.cloudCopyApprovalPhrase(candidate, "copy-only")).toBe(
      `DiskSage cloud copy-only ${"a".repeat(64)} 승인`,
    );
    expect(api.cloudCopyApprovalPhrase(candidate, "adopt-existing-copy")).toBe(
      `DiskSage cloud adopt-existing-copy ${"a".repeat(64)} 승인`,
    );
  });
});
''',
        '''describe("cloud copy approval phrase", () => {
  const exactPhrase = `DiskSage cloud copy-only ${"a".repeat(64)} 승인`;

  it("returns only the backend-authored phrase for the matching action", () => {
    const candidate = {
      copy_approval_action: "copy-only" as const,
      exact_copy_approval_phrase: exactPhrase,
    };
    expect(api.cloudCopyApprovalPhrase(candidate, "copy-only")).toBe(exactPhrase);
    expect(api.cloudCopyApprovalPhrase(candidate, "adopt-existing-copy")).toBeNull();
  });

  it("fails closed when the backend omitted the action or exact phrase", () => {
    expect(api.cloudCopyApprovalPhrase({}, "copy-only")).toBeNull();
    expect(api.cloudCopyApprovalPhrase({
      copy_approval_action: "copy-only",
      exact_copy_approval_phrase: null,
    }, "copy-only")).toBeNull();
  });
});
''',
    )
    replace_once(
        CONTRACT,
        '"/** Builds the exact confirmation phrase shown to and entered by the human reviewer. */\\nexport const cloudCopyApprovalPhrase",',
        '"/** Returns the exact backend-authored phrase only for the matching candidate action. */\\nexport const cloudCopyApprovalPhrase",',
    )
    append_once(
        CONTRACT,
        "fn cloud_plan_exports_backend_authored_approval_phrase()",
        r'''
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
''',
    )


def view_source() -> str:
    """Return the typed Rust plan-view module with complete public documentation and tests."""
    return r'''//! Backend-authored cloud-plan presentation contract.
//!
//! The core planner remains independent of approval presentation. This adapter enriches each
//! serialized candidate with the only action currently available and the exact phrase generated by
//! Rust for that action. Frontends may display the value but must not reconstruct authorization
//! text independently.

use crate::cloud::{
    CloudCandidate, CloudPlanOptions, CloudPlanReport, CloudRoot, ExactDuplicateSummary,
};
use crate::cloud_transfer::{
    cloud_copy_approval_phrase, CloudCopyApprovalAction, MAX_CLOUD_COPY_APPROVAL_AGE_MS,
};
use crate::provider_capacity::CloudCapacityAssessment;

/// One cloud candidate plus the backend-authored approval presentation for its current state.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CloudPlanCandidateView {
    /// Original candidate evidence and destination decision fields.
    #[serde(flatten)]
    pub candidate: CloudCandidate,
    /// Exact action available for this candidate, or `None` when another blocker applies.
    pub copy_approval_action: Option<CloudCopyApprovalAction>,
    /// Candidate-specific confirmation phrase generated by Rust for the available action.
    pub exact_copy_approval_phrase: Option<String>,
    /// Maximum age, in milliseconds, accepted for an approval created from this plan.
    pub copy_approval_max_age_ms: u64,
}

impl From<CloudCandidate> for CloudPlanCandidateView {
    fn from(candidate: CloudCandidate) -> Self {
        let action = match candidate.blocked_reason.as_deref() {
            None => Some(CloudCopyApprovalAction::CopyOnly),
            Some("destination-exists") => Some(CloudCopyApprovalAction::AdoptExistingCopy),
            Some(_) => None,
        };
        let exact_copy_approval_phrase =
            action.map(|action| cloud_copy_approval_phrase(&candidate, action));
        Self {
            candidate,
            copy_approval_action: action,
            exact_copy_approval_phrase,
            copy_approval_max_age_ms: MAX_CLOUD_COPY_APPROVAL_AGE_MS,
        }
    }
}

/// Serialized cloud plan consumed by the desktop UI and compatible CWL modules.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CloudPlanReportView {
    /// Destination root selected and revalidated by the planner.
    pub cloud_root: CloudRoot,
    /// Millisecond Unix timestamp at which the plan was generated.
    pub generated_at_ms: u64,
    /// Source-selection policy used to collect this bounded candidate set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_selection_policy: Option<CloudPlanOptions>,
    /// Candidate evidence enriched with backend-authored approval presentation.
    pub candidates: Vec<CloudPlanCandidateView>,
    /// Total logical bytes represented by all candidates.
    pub candidate_bytes: u64,
    /// Candidate bytes that may become locally reclaimable after all safety gates pass.
    pub potentially_reclaimable_bytes: u64,
    /// Read-only exact-duplicate analysis attached to the plan.
    pub exact_duplicates: ExactDuplicateSummary,
    /// Authenticated provider capacity evidence when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<CloudCapacityAssessment>,
    /// Stable operator notices produced by the planner and provider gates.
    pub notices: Vec<String>,
}

impl From<CloudPlanReport> for CloudPlanReportView {
    fn from(report: CloudPlanReport) -> Self {
        let CloudPlanReport {
            cloud_root,
            generated_at_ms,
            source_selection_policy,
            candidates,
            candidate_bytes,
            potentially_reclaimable_bytes,
            exact_duplicates,
            capacity,
            notices,
        } = report;
        Self {
            cloud_root,
            generated_at_ms,
            source_selection_policy,
            candidates: candidates.into_iter().map(CloudPlanCandidateView::from).collect(),
            candidate_bytes,
            potentially_reclaimable_bytes,
            exact_duplicates,
            capacity,
            notices,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::{
        ArchiveKind, CloudAccountScope, CloudProvider, MetadataEvidence,
    };

    fn candidate(blocked_reason: Option<&str>) -> CloudCandidate {
        CloudCandidate {
            metadata_fingerprint: "a".repeat(64),
            review_fingerprint: "b".repeat(64),
            src: "/source/report.pdf".into(),
            dst: "/cloud/DiskSage Archive/documents/report.pdf".into(),
            provider: CloudProvider::Icloud,
            destination_account_scope: CloudAccountScope::Personal,
            kind: ArchiveKind::Document,
            bytes: 4096,
            age_days: 120,
            created_ms: 1,
            modified_ms: 2,
            production_time_ms: 1,
            production_time_source: "embedded:pdf".into(),
            production_time_confidence: "high".into(),
            source_root: "/source".into(),
            relative_path: "report.pdf".into(),
            source_context: "source".into(),
            requires_review: false,
            review_reasons: Vec::new(),
            content_title: Some("Report".into()),
            content_authors: vec!["Analyst".into()],
            content_context: Vec::new(),
            duration_ms: None,
            dataset_profile: None,
            metadata_evidence: vec![MetadataEvidence {
                field: "title".into(),
                value: "Report".into(),
                source: "pdf-info".into(),
                confidence: "high".into(),
            }],
            blocked_reason: blocked_reason.map(str::to_owned),
        }
    }

    #[test]
    fn new_copy_candidate_exports_rust_phrase_and_lifetime() {
        let view = CloudPlanCandidateView::from(candidate(None));
        assert_eq!(
            view.copy_approval_action,
            Some(CloudCopyApprovalAction::CopyOnly)
        );
        assert_eq!(
            view.exact_copy_approval_phrase.as_deref(),
            Some(format!("DiskSage cloud copy-only {} 승인", "b".repeat(64)).as_str())
        );
        assert_eq!(
            view.copy_approval_max_age_ms,
            MAX_CLOUD_COPY_APPROVAL_AGE_MS
        );
        let serialized = serde_json::to_value(&view).unwrap();
        assert_eq!(serialized["copy_approval_action"], "copy-only");
        assert_eq!(serialized["copy_approval_max_age_ms"], 900_000);
    }

    #[test]
    fn destination_collision_exports_existing_copy_adoption_phrase() {
        let view = CloudPlanCandidateView::from(candidate(Some("destination-exists")));
        assert_eq!(
            view.copy_approval_action,
            Some(CloudCopyApprovalAction::AdoptExistingCopy)
        );
        assert_eq!(
            view.exact_copy_approval_phrase.as_deref(),
            Some(
                format!(
                    "DiskSage cloud adopt-existing-copy {} 승인",
                    "b".repeat(64)
                )
                .as_str()
            )
        );
    }

    #[test]
    fn unrelated_blocker_exports_no_authorization_text() {
        let view = CloudPlanCandidateView::from(candidate(Some("source-changed")));
        assert_eq!(view.copy_approval_action, None);
        assert_eq!(view.exact_copy_approval_phrase, None);
        let serialized = serde_json::to_value(&view).unwrap();
        assert!(serialized["copy_approval_action"].is_null());
        assert!(serialized["exact_copy_approval_phrase"].is_null());
    }

    #[test]
    fn report_conversion_preserves_plan_evidence_and_enriches_candidates() {
        let report = CloudPlanReport {
            cloud_root: CloudRoot {
                id: "icloud-personal".into(),
                provider: CloudProvider::Icloud,
                account_scope: CloudAccountScope::Personal,
                label: "iCloud Drive".into(),
                path: "/cloud".into(),
                readable: true,
                access_issue: None,
            },
            generated_at_ms: 42,
            source_selection_policy: Some(CloudPlanOptions::default()),
            candidates: vec![candidate(None)],
            candidate_bytes: 4096,
            potentially_reclaimable_bytes: 4096,
            exact_duplicates: ExactDuplicateSummary::default(),
            capacity: None,
            notices: vec!["cloud-quota-provider-native-verified".into()],
        };
        let view = CloudPlanReportView::from(report);
        assert_eq!(view.generated_at_ms, 42);
        assert_eq!(view.candidate_bytes, 4096);
        assert_eq!(view.candidates.len(), 1);
        assert_eq!(
            view.candidates[0].copy_approval_action,
            Some(CloudCopyApprovalAction::CopyOnly)
        );
        assert_eq!(
            view.notices,
            vec!["cloud-quota-provider-native-verified".to_string()]
        );
    }
}
'''


def apply_implementation() -> None:
    """Create the typed backend view and migrate every frontend consumer to it."""
    if VIEW.exists():
        raise SystemExit(f"refusing to overwrite existing file: {VIEW}")
    VIEW.write_text(view_source(), encoding="utf-8")
    replace_once(
        LIB,
        '''#[cfg_attr(coverage, allow(dead_code))]
pub mod cloud;
pub mod cloud_local_inventory;
''',
        '''#[cfg_attr(coverage, allow(dead_code))]
pub mod cloud;
/// Typed backend-authored presentation contract for cloud archive plans.
pub mod cloud_plan_view;
pub mod cloud_local_inventory;
''',
    )
    replace_once(
        COMMANDS,
        '''use crate::{
    cloud, cloud_eviction, cloud_local_eviction, cloud_review, cloud_transfer, dev_artifacts,
    dupes, git_worktree, icloud_sync_health, provider_api_client, provider_capacity,
''',
        '''use crate::{
    cloud, cloud_eviction, cloud_local_eviction, cloud_plan_view, cloud_review, cloud_transfer,
    dev_artifacts, dupes, git_worktree, icloud_sync_health, provider_api_client, provider_capacity,
''',
    )
    replace_once(
        COMMANDS,
        '''/// Read-only cloud offload plan. The selected destination must be one of the roots discovered
/// on this machine; this command never creates a folder or moves a file.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn plan_cloud_archive(
    root: String,
    cloud_root: String,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    app: AppHandle,
) -> Result<cloud::CloudPlanReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (_, report) =
            cloud_plan_for_inputs(&root, &cloud_root, min_size_mib, min_age_days, limit, &app)?;
        Ok(report)
    })
''',
        '''/// Read-only cloud offload plan. The selected destination must be one of the roots discovered
/// on this machine; this command never creates a folder or moves a file. Candidate approval text
/// is generated by Rust and returned as presentation evidence, never reconstructed by the UI.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn plan_cloud_archive(
    root: String,
    cloud_root: String,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    app: AppHandle,
) -> Result<cloud_plan_view::CloudPlanReportView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (_, report) =
            cloud_plan_for_inputs(&root, &cloud_root, min_size_mib, min_age_days, limit, &app)?;
        Ok(report.into())
    })
''',
    )
    replace_once(
        API,
        '''  metadata_evidence: MetadataEvidence[];
  blocked_reason: string | null;
}
''',
        '''  metadata_evidence: MetadataEvidence[];
  blocked_reason: string | null;
  /** Backend-selected action available for this candidate's current destination state. */
  copy_approval_action?: CloudCopyApprovalAction | null;
  /** Exact candidate-specific approval phrase generated by Rust, or null when blocked. */
  exact_copy_approval_phrase?: string | null;
  /** Maximum age in milliseconds accepted for an approval created from this plan. */
  copy_approval_max_age_ms?: number;
}
''',
    )
    replace_once(
        API,
        '''/** Builds the exact confirmation phrase shown to and entered by the human reviewer. */
export const cloudCopyApprovalPhrase = (
  candidate: Pick<CloudCandidate, "review_fingerprint">,
  action: CloudCopyApprovalAction,
) => `DiskSage cloud ${action} ${candidate.review_fingerprint} 승인`;
''',
        '''/** Returns the exact backend-authored phrase only for the matching candidate action. */
export const cloudCopyApprovalPhrase = (
  candidate: Pick<CloudCandidate, "copy_approval_action" | "exact_copy_approval_phrase">,
  action: CloudCopyApprovalAction,
): string | null => candidate.copy_approval_action === action
  ? candidate.exact_copy_approval_phrase ?? null
  : null;
''',
    )
    replace_once(
        UI,
        '''  function copyEligible(candidate: api.CloudCandidate): boolean {
    const decision = matchingReviewDecision(candidate);
    const exactApproval = decision?.disposition === "approved";
    const embeddedHighConfidence = candidate.production_time_confidence === "high"
      && candidate.production_time_source.startsWith("embedded:");
    const capacityEvidenceAvailable = api.cloudCapacityAllowsCopy(report?.capacity);
    return candidate.blocked_reason === null
      && (!candidate.requires_review || exactApproval)
      && (embeddedHighConfidence || exactApproval)
      && capacityEvidenceAvailable;
  }
''',
        '''  function copyEligible(candidate: api.CloudCandidate): boolean {
    const decision = matchingReviewDecision(candidate);
    const exactApproval = decision?.disposition === "approved";
    const embeddedHighConfidence = candidate.production_time_confidence === "high"
      && candidate.production_time_source.startsWith("embedded:");
    const capacityEvidenceAvailable = api.cloudCapacityAllowsCopy(report?.capacity);
    const approvalPhrase = api.cloudCopyApprovalPhrase(candidate, "copy-only");
    return candidate.blocked_reason === null
      && (!candidate.requires_review || exactApproval)
      && (embeddedHighConfidence || exactApproval)
      && capacityEvidenceAvailable
      && approvalPhrase !== null;
  }
''',
    )
    replace_once(
        UI,
        '''  function adoptEligible(candidate: api.CloudCandidate): boolean {
    const decision = matchingReviewDecision(candidate);
    const exactApproval = decision?.disposition === "approved";
    const embeddedHighConfidence = candidate.production_time_confidence === "high"
      && candidate.production_time_source.startsWith("embedded:");
    return candidate.blocked_reason === "destination-exists"
      && (!candidate.requires_review || exactApproval)
      && (embeddedHighConfidence || exactApproval);
  }
''',
        '''  function adoptEligible(candidate: api.CloudCandidate): boolean {
    const decision = matchingReviewDecision(candidate);
    const exactApproval = decision?.disposition === "approved";
    const embeddedHighConfidence = candidate.production_time_confidence === "high"
      && candidate.production_time_source.startsWith("embedded:");
    const approvalPhrase = api.cloudCopyApprovalPhrase(candidate, "adopt-existing-copy");
    return candidate.blocked_reason === "destination-exists"
      && (!candidate.requires_review || exactApproval)
      && (embeddedHighConfidence || exactApproval)
      && approvalPhrase !== null;
  }
''',
    )
    replace_once(
        UI,
        '''    const approvalRationale =
      (copyRationales[candidate.metadata_fingerprint] ?? "").trim();
    if (exactConfirmationPhrase !== api.cloudCopyApprovalPhrase(candidate, "copy-only")
      || !approvalRationale) return;
''',
        '''    const approvalRationale =
      (copyRationales[candidate.metadata_fingerprint] ?? "").trim();
    const expectedApprovalPhrase = api.cloudCopyApprovalPhrase(candidate, "copy-only");
    if (!expectedApprovalPhrase
      || exactConfirmationPhrase !== expectedApprovalPhrase
      || !approvalRationale) return;
''',
    )
    replace_once(
        UI,
        '''    const approvalRationale =
      (copyRationales[candidate.metadata_fingerprint] ?? "").trim();
    if (exactConfirmationPhrase
        !== api.cloudCopyApprovalPhrase(candidate, "adopt-existing-copy")
      || !approvalRationale) return;
''',
        '''    const approvalRationale =
      (copyRationales[candidate.metadata_fingerprint] ?? "").trim();
    const expectedApprovalPhrase = api.cloudCopyApprovalPhrase(
      candidate,
      "adopt-existing-copy",
    );
    if (!expectedApprovalPhrase
      || exactConfirmationPhrase !== expectedApprovalPhrase
      || !approvalRationale) return;
''',
    )
    replace_once(
        UI,
        '''            {#if copyEligible(candidate)}
              <div class="copy-approval">
                <div class="context">현재 메타데이터·출발지·목적지에 결부된 문구를 정확히 입력해야 합니다.</div>
                <code>{api.cloudCopyApprovalPhrase(candidate, "copy-only")}</code>
''',
        '''            {#if copyEligible(candidate)}
              {@const copyApprovalPhrase = api.cloudCopyApprovalPhrase(candidate, "copy-only")}
              <div class="copy-approval">
                <div class="context">현재 메타데이터·출발지·목적지에 결부된 문구를 정확히 입력해야 합니다.</div>
                <code>{copyApprovalPhrase ?? "현재 계획의 승인 문구를 확인할 수 없습니다."}</code>
''',
    )
    replace_once(
        UI,
        '''                    || !(copyRationales[candidate.metadata_fingerprint] ?? "").trim()
                    || (copyConfirmations[candidate.metadata_fingerprint] ?? "").trim()
                      !== api.cloudCopyApprovalPhrase(candidate, "copy-only")}
''',
        '''                    || !(copyRationales[candidate.metadata_fingerprint] ?? "").trim()
                    || copyApprovalPhrase === null
                    || (copyConfirmations[candidate.metadata_fingerprint] ?? "").trim()
                      !== copyApprovalPhrase}
''',
    )
    replace_once(
        UI,
        '''            {#if adoptEligible(candidate)}
              <div class="copy-approval">
                <div class="context">기존 목적지 파일의 전체 해시 검증·채택도 정확한 별도 승인이 필요합니다.</div>
                <code>{api.cloudCopyApprovalPhrase(candidate, "adopt-existing-copy")}</code>
''',
        '''            {#if adoptEligible(candidate)}
              {@const adoptApprovalPhrase = api.cloudCopyApprovalPhrase(candidate, "adopt-existing-copy")}
              <div class="copy-approval">
                <div class="context">기존 목적지 파일의 전체 해시 검증·채택도 정확한 별도 승인이 필요합니다.</div>
                <code>{adoptApprovalPhrase ?? "현재 계획의 채택 승인 문구를 확인할 수 없습니다."}</code>
''',
    )
    replace_once(
        UI,
        '''                    || !(copyRationales[candidate.metadata_fingerprint] ?? "").trim()
                    || (copyConfirmations[candidate.metadata_fingerprint] ?? "").trim()
                      !== api.cloudCopyApprovalPhrase(candidate, "adopt-existing-copy")}
''',
        '''                    || !(copyRationales[candidate.metadata_fingerprint] ?? "").trim()
                    || adoptApprovalPhrase === null
                    || (copyConfirmations[candidate.metadata_fingerprint] ?? "").trim()
                      !== adoptApprovalPhrase}
''',
    )


def verify() -> None:
    """Fail unless the tests and implementation preserve the backend-only trust boundary."""
    required_files = (VIEW, COMMANDS, LIB, CONTRACT, API, API_TEST, UI)
    missing_files = [str(path) for path in required_files if not path.exists()]
    if missing_files:
        raise SystemExit(f"required files missing: {missing_files}")
    sources = {path: path.read_text(encoding="utf-8") for path in required_files}
    required_markers = {
        VIEW: (
            "pub struct CloudPlanCandidateView",
            "pub copy_approval_action: Option<CloudCopyApprovalAction>",
            "pub exact_copy_approval_phrase: Option<String>",
            "cloud_copy_approval_phrase(&candidate, action)",
        ),
        COMMANDS: ("Result<cloud_plan_view::CloudPlanReportView, String>", "Ok(report.into())"),
        LIB: ("pub mod cloud_plan_view;",),
        API: (
            "copy_approval_action?: CloudCopyApprovalAction | null",
            "exact_copy_approval_phrase?: string | null",
            "candidate.copy_approval_action === action",
        ),
        API_TEST: ("returns only the backend-authored phrase", "fails closed when the backend omitted"),
        UI: (
            '{@const copyApprovalPhrase = api.cloudCopyApprovalPhrase(candidate, "copy-only")}',
            '{@const adoptApprovalPhrase = api.cloudCopyApprovalPhrase(candidate, "adopt-existing-copy")}',
        ),
        CONTRACT: ("fn cloud_plan_exports_backend_authored_approval_phrase()",),
    }
    missing = []
    for path, markers in required_markers.items():
        missing.extend(f"{path}:{marker}" for marker in markers if marker not in sources[path])
    if "`DiskSage cloud ${action} ${candidate.review_fingerprint} 승인`" in sources[API]:
        missing.append("frontend phrase formatter remains")
    if missing:
        raise SystemExit(f"backend phrase repair incomplete: {missing}")


def main() -> None:
    """Run the requested test or implementation phase and verify when complete."""
    parser = argparse.ArgumentParser()
    phases = parser.add_mutually_exclusive_group(required=True)
    phases.add_argument("--tests-only", action="store_true")
    phases.add_argument("--implementation-only", action="store_true")
    phases.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    if args.tests_only:
        add_tests()
    elif args.implementation_only:
        apply_implementation()
    else:
        verify()


if __name__ == "__main__":
    main()
