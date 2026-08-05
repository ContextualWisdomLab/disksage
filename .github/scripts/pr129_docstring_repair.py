#!/usr/bin/env python3
"""Apply and verify complete production docstrings for DiskSage PR 129."""

from __future__ import annotations

import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CORE = ROOT / "src-tauri/src/cloud_local_eviction_batch.rs"
CLI = ROOT / "src-tauri/src/bin/disksage-icloud-local-eviction-batch.rs"


class RepairError(RuntimeError):
    """Raised when an audited source anchor is absent, duplicated, or ambiguous."""


def insert_before(text: str, anchor: str, documentation: str, label: str) -> str:
    """Insert one documentation block before one exact source anchor."""
    expected = f"{documentation}{anchor}"
    if expected in text:
        return text
    count = text.count(anchor)
    if count != 1:
        raise RepairError(f"{label}: expected one anchor, found {count}")
    return text.replace(anchor, expected, 1)


def replace_once(text: str, old: str, new: str, label: str) -> str:
    """Replace one exact audited block while tolerating an already-applied repair."""
    if new in text:
        return text
    count = text.count(old)
    if count != 1:
        raise RepairError(f"{label}: expected one source block, found {count}")
    return text.replace(old, new, 1)


CORE_INSERTIONS = (
    (
        "const MAX_RATIONALE_BYTES: usize = 1_024;\n",
        "/// Maximum UTF-8 byte length accepted for an operator approval rationale.\n",
        "core rationale bound",
    ),
    (
        "const BATCH_NOTICES: [&str; 4] = [\n",
        "/// Stable limitations serialized into every batch plan for operator review.\n",
        "core notice contract",
    ),
    (
        "fn valid_hex64(value: &str) -> bool {\n",
        "/// Return whether `value` is exactly one 64-character hexadecimal digest.\n",
        "core digest validator",
    ),
    (
        "fn hash_field(hasher: &mut blake3::Hasher, value: &[u8]) {\n",
        "/// Add one length-delimited byte field to a BLAKE3 evidence digest.\n",
        "core hash field",
    ),
    (
        "fn batch_blockers(items: &[IcloudLocalEvictionBatchItem]) -> Vec<String> {\n",
        "/// Derive the fail-closed blocker list from the complete executable item set.\n",
        "core blocker derivation",
    ),
    (
        "fn batch_fingerprint_for(plan: &IcloudLocalEvictionBatchPlan) -> String {\n",
        "/// Compute the deterministic fingerprint that binds one complete batch plan.\n",
        "core plan fingerprint",
    ),
    (
        "fn approval_id_for(\n",
        "/// Compute the deterministic identifier for one attributed batch approval.\n",
        "core approval identifier",
    ),
    (
        "fn result_id_for(result: &IcloudLocalEvictionBatchResult) -> String {\n",
        "/// Compute the deterministic identifier for the current batch checkpoint state.\n",
        "core result identifier",
    ),
    (
        "fn bounded_error_code(error: &str) -> String {\n",
        "/// Return a bounded path-free error code, replacing unsafe diagnostics with a constant.\n",
        "core bounded diagnostic",
    ),
    (
        "fn item_plan_is_safe(plan: &IcloudLocalEvictionPlan) -> bool {\n",
        "/// Return whether a single-item plan contains every required iCloud safety proof.\n",
        "core item safety predicate",
    ),
    (
        "fn expected_notices() -> Vec<String> {\n",
        "/// Materialize the stable operator notices in their canonical serialized order.\n",
        "core notices",
    ),
    (
        "fn validate_batch_plan(\n",
        "/// Validate the shape, identity, totals, blockers, notices, and digest of a batch plan.\n",
        "core plan validator",
    ),
    (
        "fn build_batch_plan(\n",
        "/// Build and self-validate one immutable batch plan from planned and unavailable inputs.\n",
        "core plan builder",
    ),
    (
        "fn plan_batch_with<F>(\n",
        "/// Plan a bounded set of unique paths with an injected read-only single-item planner.\n",
        "core injected planner",
    ),
    (
        "fn validate_batch_approval(\n",
        "/// Validate that an attributed approval is canonical and bound to the exact current plan.\n",
        "core approval validator",
    ),
    (
        "fn preflight_with<F>(\n",
        "/// Re-plan every selected item and fail before mutation if any evidence changed.\n",
        "core preflight",
    ),
    (
        "fn refresh_result_summary(result: &mut IcloudLocalEvictionBatchResult, completed_at_ms: u64) {\n",
        "/// Recompute checkpoint counters, completion flags, byte totals, and result identity.\n",
        "core checkpoint summary",
    ),
    (
        "fn checkpoint_name(approval_id: &str, attempted_count: u32) -> String {\n",
        "/// Build the create-new filename for one batch-level execution checkpoint.\n",
        "core checkpoint filename",
    ),
    (
        "#[cfg(not(coverage))]\nstruct ImmutableBatchRecordWriter;\n",
        "/// Production recorder that delegates to the create-new immutable record writer.\n",
        "core immutable recorder",
    ),
    (
        "#[cfg(not(coverage))]\nfn fresh_item_requested_at_ms(now_ms: &mut impl FnMut() -> u64) -> u64 {\n",
        "/// Read a fresh timestamp immediately before one item execution attempt.\n",
        "core item clock",
    ),
    (
        "#[cfg(not(coverage))]\nfn execute_icloud_local_eviction_batch_with<P, E, R, N>(\n",
        "/// Coordinate one batch through private planner, executor, recorder, and clock seams.\n///\n/// These seams exist for deterministic failure-path tests; callers use\n/// [`execute_icloud_local_eviction_batch`] instead of invoking this helper directly.\n",
        "core injected coordinator",
    ),
)

OLD_TRAIT_DOC = """/// Execute one fully preflighted batch.
///
/// All current plans and all immutable individual approval records are prepared before the first
/// eviction request. Execution stops after the first error or incomplete verification. Each
/// attempted item is followed by a create-new batch checkpoint; a rerun therefore fails before a
/// mutation instead of silently reusing an earlier approval record.
#[cfg(not(coverage))]
trait BatchRecordWriter {
"""
NEW_TRAIT_DOC = """/// Abstracts create-new immutable record persistence for the batch coordinator.
///
/// The production implementation writes through `write_immutable_record`. Tests inject a recorder
/// that records filenames and deterministic failures without touching a user's filesystem.
#[cfg(not(coverage))]
trait BatchRecordWriter {
    /// Serialize `value` into one newly created immutable record named `name`.
"""

OLD_IMPL_METHOD = """impl BatchRecordWriter for ImmutableBatchRecordWriter {
    fn write<T: serde::Serialize>(
"""
NEW_IMPL_METHOD = """impl BatchRecordWriter for ImmutableBatchRecordWriter {
    /// Persist one record through the production create-new immutable writer.
    fn write<T: serde::Serialize>(
"""

CLI_INSERTIONS = (
    (
        "const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;\n",
        "/// Maximum manifest size read from disk before JSON parsing is rejected.\n",
        "cli manifest byte bound",
    ),
    (
        "const HELP_REQUESTED: &str = \"icloud-local-eviction-batch-help-requested\";\n",
        "/// Internal sentinel used to distinguish a successful help request from an error.\n",
        "cli help sentinel",
    ),
    (
        "#[derive(Debug, Clone, PartialEq, Eq)]\nstruct Args {\n",
        "/// Validated command-line inputs for read-only planning or explicitly approved execution.\n",
        "cli arguments",
    ),
    (
        "fn usage() -> &'static str {\n",
        "/// Return the stable, path-free command-line usage string.\n",
        "cli usage",
    ),
    (
        "fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {\n",
        "/// Read the value following `flag` without echoing a missing or sensitive argument.\n",
        "cli value reader",
    ),
    (
        "fn parse_args(args: &[String]) -> Result<Args, String> {\n",
        "/// Parse arguments and enforce complete all-or-nothing execution authorization fields.\n",
        "cli parser",
    ),
    (
        "fn home_dir() -> Result<PathBuf, String> {\n",
        "/// Return an absolute HOME directory or a stable fail-closed diagnostic.\n",
        "cli home directory",
    ),
    (
        "fn canonical_existing(path: &Path, error_code: &str) -> Result<PathBuf, String> {\n",
        "/// Canonicalize an existing control path while replacing system errors with a stable code.\n",
        "cli canonical path",
    ),
    (
        "fn paths_overlap(left: &Path, right: &Path) -> bool {\n",
        "/// Return whether either canonical path is equal to or contains the other.\n",
        "cli overlap predicate",
    ),
    (
        "fn validate_control_locations(\n",
        "/// Ensure the manifest and immutable-record directory cannot overlap protected cloud data.\n",
        "cli control locations",
    ),
    (
        "fn select_root<'a>(roots: &'a [CloudRoot], requested: &Path) -> Result<&'a CloudRoot, String> {\n",
        "/// Select exactly one detected iCloud root matching the requested canonical path.\n",
        "cli root selector",
    ),
    (
        "#[derive(Debug, Deserialize)]\nstruct InputManifest {\n",
        "/// Minimal JSON manifest accepted by the batch command.\n",
        "cli input manifest",
    ),
    (
        "#[derive(Debug, Deserialize)]\nstruct InputManifestItem {\n",
        "/// One absolute candidate path supplied by the input manifest.\n",
        "cli input item",
    ),
    (
        "fn read_manifest_paths(path: &Path) -> Result<Vec<PathBuf>, String> {\n",
        "/// Read a bounded regular-file manifest and return validated absolute candidate paths.\n",
        "cli manifest reader",
    ),
    (
        "#[derive(Debug, serde::Serialize)]\nstruct RedactedBatchPlan {\n",
        "/// Path-free batch-plan view printed for operator review.\n",
        "cli redacted plan",
    ),
    (
        "fn redact_plan(plan: &IcloudLocalEvictionBatchPlan) -> RedactedBatchPlan {\n",
        "/// Convert a complete internal plan into the path-free operator output contract.\n",
        "cli plan redactor",
    ),
    (
        "#[derive(Debug, serde::Serialize)]\nstruct PlanOutput {\n",
        "/// Top-level JSON response for a read-only batch planning request.\n",
        "cli plan output",
    ),
    (
        "#[derive(Debug, serde::Serialize)]\nstruct RedactedBatchResult {\n",
        "/// Path-free execution-result view suitable for logs and automation.\n",
        "cli redacted result",
    ),
    (
        "fn redact_result(result: &IcloudLocalEvictionBatchResult) -> RedactedBatchResult {\n",
        "/// Convert an internal execution result into the path-free output contract.\n",
        "cli result redactor",
    ),
    (
        "#[derive(Debug, serde::Serialize)]\nstruct ExecuteOutput {\n",
        "/// Top-level JSON response for an explicitly approved execution request.\n",
        "cli execution output",
    ),
    (
        "fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {\n",
        "/// Serialize one response as deterministic pretty JSON on standard output.\n",
        "cli JSON printer",
    ),
    (
        "fn run() -> Result<(), String> {\n",
        "/// Execute one command invocation from validated arguments through redacted output.\n",
        "cli runner",
    ),
    (
        "fn main() {\n",
        "/// Process one command invocation and map operational failures to exit status 2.\n",
        "cli entrypoint",
    ),
)


def apply_repair(path: Path, insertions: tuple[tuple[str, str, str], ...]) -> None:
    """Apply all audited insertions to one source file."""
    text = path.read_text(encoding="utf-8")
    for anchor, documentation, label in insertions:
        text = insert_before(text, anchor, documentation, label)
    path.write_text(text, encoding="utf-8")


def verify_insertions(path: Path, insertions: tuple[tuple[str, str, str], ...]) -> list[str]:
    """Return labels for documentation blocks that are not present exactly once."""
    text = path.read_text(encoding="utf-8")
    missing: list[str] = []
    for anchor, documentation, label in insertions:
        if text.count(f"{documentation}{anchor}") != 1:
            missing.append(label)
    return missing


def apply() -> None:
    """Apply the bounded documentation repair to the two audited Rust sources."""
    core = CORE.read_text(encoding="utf-8")
    core = replace_once(core, OLD_TRAIT_DOC, NEW_TRAIT_DOC, "core recorder trait documentation")
    core = replace_once(core, OLD_IMPL_METHOD, NEW_IMPL_METHOD, "core recorder implementation method")
    CORE.write_text(core, encoding="utf-8")
    apply_repair(CORE, CORE_INSERTIONS)
    apply_repair(CLI, CLI_INSERTIONS)


def verify() -> None:
    """Fail unless every audited production declaration has its beginner-readable docstring."""
    failures = verify_insertions(CORE, CORE_INSERTIONS)
    failures.extend(verify_insertions(CLI, CLI_INSERTIONS))
    core = CORE.read_text(encoding="utf-8")
    if core.count(NEW_TRAIT_DOC) != 1:
        failures.append("core recorder trait documentation")
    if core.count(NEW_IMPL_METHOD) != 1:
        failures.append("core recorder implementation method")
    if failures:
        raise SystemExit("missing production docstrings: " + ", ".join(failures))
    print(
        "production docstring evidence: "
        f"{len(CORE_INSERTIONS) + len(CLI_INSERTIONS) + 2}/"
        f"{len(CORE_INSERTIONS) + len(CLI_INSERTIONS) + 2}"
    )


def main() -> None:
    """Parse the requested operation and run the deterministic repair or verifier."""
    parser = argparse.ArgumentParser()
    parser.add_argument("operation", choices=("apply", "verify"))
    args = parser.parse_args()
    try:
        if args.operation == "apply":
            apply()
        else:
            verify()
    except RepairError as error:
        raise SystemExit(str(error)) from error


if __name__ == "__main__":
    main()
