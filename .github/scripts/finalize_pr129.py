#!/usr/bin/env python3
"""Apply and clean up the bounded PR 129 coverage and documentation repair."""

from __future__ import annotations

import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE_PATH = ROOT / "src-tauri/src/cloud_local_eviction_batch.rs"
TEST_PATH = ROOT / "src-tauri/tests/icloud_local_eviction_batch_documentation_test.rs"
TRIGGER_PATH = ROOT / ".github/repair-pr129-trigger.txt"
WORKFLOW_PATH = ROOT / ".github/workflows/repair-pr129-coverage-seams.yml"
SCRIPT_PATH = Path(__file__).resolve()

REGRESSION_NAME = "batch_module_documents_every_production_function_and_support_item"
REGRESSION_TEST = r'''

/// Verifies that every production helper and injected support item retains attached Rust docs.
#[test]
fn batch_module_documents_every_production_function_and_support_item() {
    let source = include_str!("../src/cloud_local_eviction_batch.rs");
    let production = source
        .split("\n#[cfg(test)]\nmod tests {")
        .next()
        .expect("production module prefix");
    let lines: Vec<&str> = production.lines().collect();
    let mut undocumented = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let indentation = line.len().saturating_sub(trimmed.len());
        let production_item = (indentation == 0
            && (trimmed.starts_with("fn ")
                || trimmed.starts_with("pub fn ")
                || trimmed.starts_with("trait BatchRecordWriter")
                || trimmed.starts_with("struct ImmutableBatchRecordWriter")))
            || (indentation == 4 && trimmed.starts_with("fn write<"));
        if !production_item {
            continue;
        }

        let mut cursor = index;
        let mut documented = false;
        while cursor > 0 {
            cursor -= 1;
            let previous = lines[cursor].trim();
            if previous.starts_with("#[") {
                continue;
            }
            documented = previous.starts_with("///");
            break;
        }
        if !documented {
            undocumented.push(trimmed.to_string());
        }
    }

    assert!(
        undocumented.is_empty(),
        "production items missing attached Rust documentation: {undocumented:?}"
    );
}
'''


def replace_once(text: str, old: str, new: str) -> str:
    """Replace one audited source fragment or fail without writing."""
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one audited fragment {old!r}, found {count}")
    return text.replace(old, new, 1)


def add_regression_test() -> None:
    """Append the documentation regression before changing production code."""
    source = TEST_PATH.read_text(encoding="utf-8")
    if REGRESSION_NAME in source:
        raise SystemExit("documentation regression already exists")
    TEST_PATH.write_text(source + REGRESSION_TEST, encoding="utf-8")


def apply_repair() -> None:
    """Expose test seams during coverage builds and document production helpers."""
    source = SOURCE_PATH.read_text(encoding="utf-8")
    source = replace_once(
        source,
        """use crate::cloud_local_eviction::{
    approve_icloud_local_eviction, execute_icloud_local_eviction, plan_icloud_local_eviction,
    write_immutable_record, IcloudLocalEvictionApproval, IcloudLocalEvictionPlan,
    IcloudLocalEvictionResult,
};
""",
        """use crate::cloud_local_eviction::{
    approve_icloud_local_eviction, plan_icloud_local_eviction, IcloudLocalEvictionApproval,
    IcloudLocalEvictionPlan, IcloudLocalEvictionResult,
};
#[cfg(not(coverage))]
use crate::cloud_local_eviction::{execute_icloud_local_eviction, write_immutable_record};
""",
    )
    for old, new in [
        (
            "#[cfg(not(coverage))]\ntrait BatchRecordWriter",
            "#[cfg(any(not(coverage), test))]\ntrait BatchRecordWriter",
        ),
        (
            "#[cfg(not(coverage))]\nfn fresh_item_requested_at_ms",
            "#[cfg(any(not(coverage), test))]\nfn fresh_item_requested_at_ms",
        ),
        (
            "#[cfg(not(coverage))]\nfn execute_icloud_local_eviction_batch_with",
            "#[cfg(any(not(coverage), test))]\nfn execute_icloud_local_eviction_batch_with",
        ),
    ]:
        source = replace_once(source, old, new)

    documentation = {
        "fn valid_hex64(value: &str) -> bool {":
            "/// Returns whether a value is a complete 64-digit hexadecimal digest.\n",
        "fn hash_field(hasher: &mut blake3::Hasher, value: &[u8]) {":
            "/// Appends one length-prefixed field to an integrity digest.\n",
        "fn batch_blockers(items: &[IcloudLocalEvictionBatchItem]) -> Vec<String> {":
            "/// Derives fail-closed batch blockers from executable item evidence.\n",
        "fn batch_fingerprint_for(plan: &IcloudLocalEvictionBatchPlan) -> String {":
            "/// Computes the deterministic digest binding a complete batch plan.\n",
        "fn approval_id_for(\n":
            "/// Computes the deterministic identifier for one attributed approval.\n",
        "fn result_id_for(result: &IcloudLocalEvictionBatchResult) -> String {":
            "/// Computes the deterministic identifier for a batch checkpoint state.\n",
        "fn bounded_error_code(error: &str) -> String {":
            "/// Converts an internal failure into a bounded path-free diagnostic code.\n",
        "fn item_plan_is_safe(plan: &IcloudLocalEvictionPlan) -> bool {":
            "/// Verifies every single-item safety invariant required by a batch.\n",
        "fn expected_notices() -> Vec<String> {":
            "/// Returns the stable operator notices serialized into every batch record.\n",
        "fn validate_batch_plan(\n":
            "/// Validates structure, totals, ordering, scope, and digest integrity.\n",
        "fn build_batch_plan(\n":
            "/// Builds and integrity-checks a normalized batch plan from bounded evidence.\n",
        "fn plan_batch_with<F>(\n":
            "/// Plans a bounded manifest through an injected read-only item planner.\n",
        "fn validate_batch_approval(\n":
            "/// Validates exact-plan confirmation, attribution, freshness, and integrity.\n",
        "fn preflight_with<F>(\n":
            "/// Replans every selected item before the first mutation and rejects drift.\n",
        "fn refresh_result_summary(result: &mut IcloudLocalEvictionBatchResult, completed_at_ms: u64) {":
            "/// Recomputes checkpoint counts, completion flags, and the result digest.\n",
        "fn checkpoint_name(approval_id: &str, attempted_count: u32) -> String {":
            "/// Derives the immutable checkpoint filename for an attempted-item count.\n",
    }
    for anchor, prefix in documentation.items():
        source = replace_once(source, anchor, prefix + anchor)

    for old, new in [
        (
            "trait BatchRecordWriter {\n    fn write",
            "trait BatchRecordWriter {\n    /// Persists one create-new immutable approval, result, or checkpoint record.\n    fn write",
        ),
        (
            "#[cfg(not(coverage))]\nstruct ImmutableBatchRecordWriter;",
            "/// Production writer that delegates to the create-new immutable record boundary.\n#[cfg(not(coverage))]\nstruct ImmutableBatchRecordWriter;",
        ),
        (
            "impl BatchRecordWriter for ImmutableBatchRecordWriter {\n    fn write",
            "impl BatchRecordWriter for ImmutableBatchRecordWriter {\n    /// Writes a serialized record without overwriting an existing evidence object.\n    fn write",
        ),
        (
            "#[cfg(any(not(coverage), test))]\nfn fresh_item_requested_at_ms",
            "#[cfg(any(not(coverage), test))]\n/// Reads a fresh mutation timestamp instead of deriving synthetic item times.\nfn fresh_item_requested_at_ms",
        ),
        (
            "#[cfg(any(not(coverage), test))]\nfn execute_icloud_local_eviction_batch_with",
            "#[cfg(any(not(coverage), test))]\n/// Executes one approved batch through injected planner, executor, writer, and clock seams.\nfn execute_icloud_local_eviction_batch_with",
        ),
    ]:
        source = replace_once(source, old, new)

    SOURCE_PATH.write_text(source, encoding="utf-8")


def cleanup() -> None:
    """Remove one-shot repair inputs after exact-head validation succeeds."""
    for path in (TRIGGER_PATH, WORKFLOW_PATH, SCRIPT_PATH):
        if not path.exists():
            raise SystemExit(f"expected one-shot path to exist: {path}")
    TRIGGER_PATH.unlink()
    WORKFLOW_PATH.unlink()
    SCRIPT_PATH.unlink()


def main() -> None:
    """Dispatch one bounded repair phase selected by the workflow."""
    parser = argparse.ArgumentParser()
    parser.add_argument("phase", choices=("add-test", "apply-repair", "cleanup"))
    args = parser.parse_args()
    if args.phase == "add-test":
        add_regression_test()
    elif args.phase == "apply-repair":
        apply_repair()
    else:
        cleanup()


if __name__ == "__main__":
    main()
