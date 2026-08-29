#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
helper="$repo_root/.github/scripts/reclaim-test-runner-space.sh"
work_dir="$(mktemp -d)"
trap 'rm -rf -- "$work_dir"' EXIT

fixture_root="$work_dir/fixture"
mkdir -p "$fixture_root/sdk-root"
dd if=/dev/zero of="$fixture_root/sdk-root/payload.bin" bs=4096 count=16 status=none
summary_file="$work_dir/summary.md"

DISKSAGE_RECLAIM_TEST_ROOT="$fixture_root" \
GITHUB_STEP_SUMMARY="$summary_file" \
  bash "$helper" "$fixture_root/sdk-root"

test ! -e "$fixture_root/sdk-root"
reclaimed_bytes="$(sed -n 's/^runner_reclaimed_bytes=//p' "$summary_file")"
if ! [[ "$reclaimed_bytes" =~ ^[1-9][0-9]*$ ]]; then
  echo "expected positive runner_reclaimed_bytes, got '${reclaimed_bytes:-missing}'" >&2
  exit 1
fi

missing_summary="$work_dir/missing-summary.md"
if DISKSAGE_RECLAIM_TEST_ROOT="$fixture_root" \
   GITHUB_STEP_SUMMARY="$missing_summary" \
   bash "$helper" "$fixture_root/missing-root"; then
  echo "expected an absent reclaim root to fail closed" >&2
  exit 1
fi

mkdir -p "$fixture_root/real-root"
ln -s "$fixture_root/real-root" "$fixture_root/link-root"
symlink_summary="$work_dir/symlink-summary.md"
if DISKSAGE_RECLAIM_TEST_ROOT="$fixture_root" \
   GITHUB_STEP_SUMMARY="$symlink_summary" \
   bash "$helper" "$fixture_root/link-root"; then
  echo "expected a symlink reclaim root to fail closed" >&2
  exit 1
fi

echo "runner reclaim positive-evidence regression passed"
