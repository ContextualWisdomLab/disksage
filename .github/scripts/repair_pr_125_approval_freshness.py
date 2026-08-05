#!/usr/bin/env python3
"""Apply the exact PR 125 live-clock approval freshness regression and repair."""

from __future__ import annotations

import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TRANSFER = ROOT / "src-tauri/src/cloud_transfer.rs"
COMMANDS = ROOT / "src-tauri/src/commands.rs"
CLI = ROOT / "src-tauri/src/bin/disksage-cloud-plan.rs"


def replace_once(path: Path, old: str, new: str) -> None:
    """Replace exactly one audited source fragment or fail without a partial write."""

    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected one audited fragment in {path}, found {count}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def add_tests() -> None:
    """Add the failing live-clock regression before changing production code."""

    anchor = """    #[test]
    fn copy_approval_requires_exact_phrase_human_attribution_context_and_freshness() {
"""
    test = """    #[test]
    #[cfg(not(coverage))]
    fn production_copy_entrypoints_recheck_approval_age_against_live_time() {
        let candidate = candidate();
        let root = root();
        let copy_approval = test_copy_approval(
            &candidate,
            &root,
            CloudCopyApprovalAction::CopyOnly,
            1,
        )
        .unwrap();
        assert_eq!(
            prepare_cloud_copy_with_approval(
                &candidate,
                &root,
                std::path::Path::new("/unused"),
                None,
                &copy_approval,
            )
            .unwrap_err(),
            "cloud-copy-approval-stale"
        );

        let adoption_approval = test_copy_approval(
            &candidate,
            &root,
            CloudCopyApprovalAction::AdoptExistingCopy,
            1,
        )
        .unwrap();
        assert_eq!(
            adopt_existing_cloud_copy_with_approval(
                &candidate,
                &root,
                std::path::Path::new("/unused"),
                None,
                &adoption_approval,
            )
            .unwrap_err(),
            "cloud-copy-approval-stale"
        );
    }

"""
    replace_once(TRANSFER, anchor, test + anchor)


def patch_transfer_entrypoints() -> None:
    """Make production entrypoints obtain a fresh clock reading at the mutation boundary."""

    old_copy = """/// Copy a candidate only after validating both the optional metadata review decision and a fresh,
/// exact, human-attributed copy approval. Every path/provider/planner gate remains mandatory.
#[cfg(not(coverage))]
pub fn prepare_cloud_copy_with_approval(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    copied_at_ms: u64,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    validate_cloud_copy_approval_for_action(
"""
    new_copy = """/// Copy a candidate only after validating both the optional metadata review decision and a fresh,
/// exact, human-attributed copy approval. The production entrypoint reads the live clock at the
/// mutation boundary so an earlier preflight cannot silently extend the approval lifetime.
#[cfg(not(coverage))]
pub fn prepare_cloud_copy_with_approval(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    prepare_cloud_copy_with_approval_at(
        candidate,
        cloud_root,
        receipt_dir,
        crate::cloud::system_now_ms(),
        review_decision,
        copy_approval,
    )
}

#[cfg(not(coverage))]
fn prepare_cloud_copy_with_approval_at(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    copied_at_ms: u64,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    validate_cloud_copy_approval_for_action(
"""
    replace_once(TRANSFER, old_copy, new_copy)

    old_adopt = """/// Verify and adopt an existing destination only after the same exact human action approval.
#[cfg(not(coverage))]
pub fn adopt_existing_cloud_copy_with_approval(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    verified_at_ms: u64,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    validate_cloud_copy_approval_for_action(
"""
    new_adopt = """/// Verify and adopt an existing destination only after the same exact human action approval.
/// The approval age is evaluated from a fresh live-clock read immediately before verification.
#[cfg(not(coverage))]
pub fn adopt_existing_cloud_copy_with_approval(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    adopt_existing_cloud_copy_with_approval_at(
        candidate,
        cloud_root,
        receipt_dir,
        crate::cloud::system_now_ms(),
        review_decision,
        copy_approval,
    )
}

#[cfg(not(coverage))]
fn adopt_existing_cloud_copy_with_approval_at(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    verified_at_ms: u64,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    validate_cloud_copy_approval_for_action(
"""
    replace_once(TRANSFER, old_adopt, new_adopt)

    old_test_copy = """    prepare_cloud_copy_with_approval(
        candidate,
        cloud_root,
        receipt_dir,
        copied_at_ms,
        review_decision,
        &approval,
    )
"""
    new_test_copy = """    prepare_cloud_copy_with_approval_at(
        candidate,
        cloud_root,
        receipt_dir,
        copied_at_ms,
        review_decision,
        &approval,
    )
"""
    replace_once(TRANSFER, old_test_copy, new_test_copy)

    old_test_adopt = """    adopt_existing_cloud_copy_with_approval(
        candidate,
        cloud_root,
        receipt_dir,
        verified_at_ms,
        None,
        &approval,
    )
"""
    new_test_adopt = """    adopt_existing_cloud_copy_with_approval_at(
        candidate,
        cloud_root,
        receipt_dir,
        verified_at_ms,
        None,
        &approval,
    )
"""
    replace_once(TRANSFER, old_test_adopt, new_test_adopt)


def patch_call_site(path: Path) -> None:
    """Remove the stale preflight timestamp from one production call site."""

    text = path.read_text(encoding="utf-8")
    for receipt_dir in ("receipt_dir", "&receipt_dir"):
        copy_fragment = f"""        cloud_transfer::prepare_cloud_copy_with_approval(
            candidate,
            &selected,
            {receipt_dir},
            action_at_ms,
            review_decision.as_ref(),
            &copy_approval,
        )?
"""
        if copy_fragment in text:
            text = text.replace(
                copy_fragment,
                f"""        cloud_transfer::prepare_cloud_copy_with_approval(
            candidate,
            &selected,
            {receipt_dir},
            review_decision.as_ref(),
            &copy_approval,
        )?
""",
                1,
            )
            break
    else:
        raise SystemExit(f"copy call fragment not found in {path}")

    for receipt_dir in ("receipt_dir", "&receipt_dir"):
        adopt_fragment = f"""        cloud_transfer::adopt_existing_cloud_copy_with_approval(
            candidate,
            &selected,
            {receipt_dir},
            action_at_ms,
            review_decision.as_ref(),
            &copy_approval,
        )?
"""
        if adopt_fragment in text:
            text = text.replace(
                adopt_fragment,
                f"""        cloud_transfer::adopt_existing_cloud_copy_with_approval(
            candidate,
            &selected,
            {receipt_dir},
            review_decision.as_ref(),
            &copy_approval,
        )?
""",
                1,
            )
            break
    else:
        raise SystemExit(f"adoption call fragment not found in {path}")

    path.write_text(text, encoding="utf-8")


def patch_production() -> None:
    """Apply the minimal production repair after the regression is proven red."""

    patch_transfer_entrypoints()
    patch_call_site(COMMANDS)
    patch_call_site(CLI)


def verify_final_shape() -> None:
    """Fail closed unless the exact live-clock API and regression are present."""

    transfer = TRANSFER.read_text(encoding="utf-8")
    commands = COMMANDS.read_text(encoding="utf-8")
    cli = CLI.read_text(encoding="utf-8")
    required = [
        "production_copy_entrypoints_recheck_approval_age_against_live_time",
        "fn prepare_cloud_copy_with_approval_at(",
        "fn adopt_existing_cloud_copy_with_approval_at(",
        "crate::cloud::system_now_ms(),",
    ]
    for marker in required:
        if marker not in transfer:
            raise SystemExit(f"missing final transfer marker: {marker}")
    for source, name in ((commands, "commands"), (cli, "cli")):
        for call in (
            "prepare_cloud_copy_with_approval(",
            "adopt_existing_cloud_copy_with_approval(",
        ):
            start = source.index(call)
            fragment = source[start : start + 300]
            if "action_at_ms," in fragment:
                raise SystemExit(f"{name} still reuses the approval timestamp in {call}")


def main() -> None:
    """Run one bounded repair stage."""

    parser = argparse.ArgumentParser()
    parser.add_argument("stage", choices=("tests", "production", "all"))
    args = parser.parse_args()
    if args.stage in {"tests", "all"}:
        add_tests()
    if args.stage in {"production", "all"}:
        patch_production()
    if args.stage == "all":
        verify_final_shape()


if __name__ == "__main__":
    main()
