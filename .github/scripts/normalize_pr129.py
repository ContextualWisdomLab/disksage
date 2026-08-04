#!/usr/bin/env python3
"""Normalize the audited PR 89 repair script for the exact current source state.

The original one-shot script contains two stale transformations whose target
source changes are already present on the branch. This helper removes only the
obsolete loop rewrite and narrows the timestamp rewrite to the remaining
executor seam. Every transformation is exact-count and fail-closed.
"""

from __future__ import annotations

from pathlib import Path

REPAIR_SCRIPT = Path(__file__).with_name("repair_pr_89.py")


def replace_exactly(text: str, old: str, new: str, label: str) -> str:
    """Replace one exact script fragment or terminate without writing."""
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one fragment, found {count}")
    return text.replace(old, new, 1)


def main() -> None:
    """Apply exact current-head normalization to the audited repair script."""
    text = REPAIR_SCRIPT.read_text(encoding="utf-8")

    old_raw = r'br#"{\\"plans\\":[]}"#'
    new_raw = r'br#"{\"plans\":[]}"#'
    if text.count(old_raw) != 2:
        raise SystemExit("raw-string anchors changed unexpectedly")
    text = text.replace(old_raw, new_raw)

    text = replace_exactly(
        text,
        r"        write_immutable_record(record_dir, name, value)\n",
        r"        write_immutable_record(record_dir, name, value).map(|_| ())\n",
        "immutable-writer anchor",
    )

    stale_loop_repair = "\n".join(
        (
            "    replace_once(",
            "        CORE,",
            "        '''    for (offset, (item, individual)) in plan\\n        .items\\n        .iter()\\n        .zip(individual_approvals.iter())\\n        .enumerate()\\n    {\\n''',",
            "        '''    for (item, individual) in plan.items.iter().zip(individual_approvals.iter()) {\\n''',",
            "    )",
            "",
        )
    )
    text = replace_exactly(text, stale_loop_repair, "", "stale loop repair")

    stale_timestamp_repair = "\n".join(
        (
            "    replace_once(",
            "        CORE,",
            "        '''        let item_requested_at_ms =\\n            requested_at_ms.saturating_add(u64::try_from(offset).unwrap_or(u64::MAX));\\n        let execution = execute_icloud_local_eviction(\\n''',",
            "        '''        let item_requested_at_ms = fresh_item_requested_at_ms(&mut now_ms);\\n        let execution = executor(\\n''',",
            "    )",
            "",
        )
    )
    current_executor_repair = "\n".join(
        (
            "    replace_once(",
            "        CORE,",
            "        '''        let execution = execute_icloud_local_eviction(\\n''',",
            "        '''        let execution = executor(\\n''',",
            "    )",
            "",
        )
    )
    text = replace_exactly(
        text,
        stale_timestamp_repair,
        current_executor_repair,
        "stale timestamp repair",
    )

    if old_raw in text or text.count(new_raw) != 2:
        raise SystemExit("raw-string normalization was not exact")
    if text.count(current_executor_repair) != 1:
        raise SystemExit("executor normalization was not exact")

    REPAIR_SCRIPT.write_text(text, encoding="utf-8")


if __name__ == "__main__":
    main()
