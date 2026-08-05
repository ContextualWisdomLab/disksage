#!/usr/bin/env python3
"""Apply and verify the exact live-clock approval-boundary repair for PR 125."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TRANSFER = ROOT / "src-tauri/src/cloud_transfer.rs"
COMMANDS = ROOT / "src-tauri/src/commands.rs"
CLI = ROOT / "src-tauri/src/bin/disksage-cloud-plan.rs"

TEST_ANCHOR = """    #[test]
    fn copy_approval_requires_exact_phrase_human_attribution_context_and_freshness() {
"""

REGRESSION_TEST = """    #[test]
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

OLD_COPY = """/// Copy a candidate only after validating both the optional metadata review decision and a fresh,
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

NEW_COPY = """/// Copy a candidate only after validating both the optional metadata review decision and a fresh,
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

OLD_ADOPT = """/// Verify and adopt an existing destination only after the same exact human action approval.
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

NEW_ADOPT = """/// Verify and adopt an existing destination only after the same exact human action approval.
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

OLD_TEST_COPY = """    prepare_cloud_copy_with_approval(
        candidate,
        cloud_root,
        receipt_dir,
        copied_at_ms,
        review_decision,
        &approval,
    )
"""

NEW_TEST_COPY = """    prepare_cloud_copy_with_approval_at(
        candidate,
        cloud_root,
        receipt_dir,
        copied_at_ms,
        review_decision,
        &approval,
    )
"""

OLD_TEST_ADOPT = """    adopt_existing_cloud_copy_with_approval(
        candidate,
        cloud_root,
        receipt_dir,
        verified_at_ms,
        None,
        &approval,
    )
"""

NEW_TEST_ADOPT = """    adopt_existing_cloud_copy_with_approval_at(
        candidate,
        cloud_root,
        receipt_dir,
        verified_at_ms,
        None,
        &approval,
    )
"""


def replace_once(text: str, old: str, new: str, label: str) -> str:
    """Replace one exact audited fragment or fail without writing partial output."""
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one audited fragment, found {count}")
    return text.replace(old, new, 1)


def add_tests() -> None:
    """Insert the failing public-entrypoint regression before production changes."""
    text = TRANSFER.read_text(encoding="utf-8")
    if REGRESSION_TEST in text:
        raise SystemExit("regression test unexpectedly already present")
    text = replace_once(text, TEST_ANCHOR, REGRESSION_TEST + TEST_ANCHOR, "regression anchor")
    TRANSFER.write_text(text, encoding="utf-8")


def remove_stale_call_timestamp(path: Path) -> None:
    """Remove the caller-supplied timestamp from both production action calls."""
    text = path.read_text(encoding="utf-8")
    for function_name in (
        "adopt_existing_cloud_copy_with_approval",
        "prepare_cloud_copy_with_approval",
    ):
        marker = f"cloud_transfer::{function_name}("
        start = text.index(marker)
        end = text.index("&copy_approval,", start)
        fragment = text[start:end]
        pattern = re.compile(r"(?m)^[ \t]+action_at_ms,\n")
        if len(pattern.findall(fragment)) != 1:
            raise SystemExit(f"{path}: expected one stale timestamp in {function_name}")
        fragment = pattern.sub("", fragment, count=1)
        text = text[:start] + fragment + text[end:]
    path.write_text(text, encoding="utf-8")


def patch_production() -> None:
    """Move freshness validation and receipt time to a live mutation-boundary clock read."""
    text = TRANSFER.read_text(encoding="utf-8")
    text = replace_once(text, OLD_COPY, NEW_COPY, "copy entrypoint")
    text = replace_once(text, OLD_ADOPT, NEW_ADOPT, "adoption entrypoint")
    text = replace_once(text, OLD_TEST_COPY, NEW_TEST_COPY, "test copy wrapper")
    text = replace_once(text, OLD_TEST_ADOPT, NEW_TEST_ADOPT, "test adoption wrapper")
    TRANSFER.write_text(text, encoding="utf-8")
    remove_stale_call_timestamp(COMMANDS)
    remove_stale_call_timestamp(CLI)


def verify() -> None:
    """Fail closed unless the exact final API and regression are present."""
    transfer = TRANSFER.read_text(encoding="utf-8")
    commands = COMMANDS.read_text(encoding="utf-8")
    cli = CLI.read_text(encoding="utf-8")
    for marker in (
        "production_copy_entrypoints_recheck_approval_age_against_live_time",
        "fn prepare_cloud_copy_with_approval_at(",
        "fn adopt_existing_cloud_copy_with_approval_at(",
        "crate::cloud::system_now_ms(),",
    ):
        if marker not in transfer:
            raise SystemExit(f"missing final marker: {marker}")
    for source, name in ((commands, "commands"), (cli, "cli")):
        for function_name in (
            "adopt_existing_cloud_copy_with_approval",
            "prepare_cloud_copy_with_approval",
        ):
            start = source.index(f"cloud_transfer::{function_name}(")
            fragment = source[start : start + 320]
            if "action_at_ms," in fragment:
                raise SystemExit(f"{name} still passes stale time to {function_name}")


def main() -> None:
    """Run one audited repair stage."""
    parser = argparse.ArgumentParser()
    parser.add_argument("stage", choices=("tests", "production", "verify"))
    args = parser.parse_args()
    if args.stage == "tests":
        add_tests()
    elif args.stage == "production":
        patch_production()
    else:
        verify()


if __name__ == "__main__":
    main()
