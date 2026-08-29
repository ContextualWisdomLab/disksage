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

# Make filesystem-capacity evidence deterministic while still exercising a real
# fixture deletion. The fake df deliberately reports no change for `/`, so a
# helper that measures the global root filesystem instead of the fixture's own
# filesystem remains RED. The requested-root filesystem reports a positive
# delta only after the real fixture root is gone.
fake_bin="$work_dir/bin"
mkdir -p "$fake_bin"
cat > "$fake_bin/df" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
last_arg="${!#}"
if [[ "$*" == *"--output=target,avail"* ]]; then
  printf 'Mounted on Avail\n'
  if [[ "$last_arg" == "$DISKSAGE_TEST_RECLAIM_ROOT" && -d "$DISKSAGE_TEST_RECLAIM_ROOT" ]]; then
    printf '%s %s\n' "$DISKSAGE_TEST_RECLAIM_MOUNT" 1048576
    exit 0
  fi
  if [[ "$last_arg" == "$DISKSAGE_TEST_RECLAIM_MOUNT" && ! -e "$DISKSAGE_TEST_RECLAIM_ROOT" ]]; then
    printf '%s %s\n' "$DISKSAGE_TEST_RECLAIM_MOUNT" 2097152
    exit 0
  fi
  exit 91
fi
if [[ "$*" == *"--output=avail"* && "$last_arg" == "/" ]]; then
  printf 'Avail\n1048576\n'
  exit 0
fi
exit 92
EOF
chmod +x "$fake_bin/df"

DISKSAGE_RECLAIM_TEST_ROOT="$fixture_root" \
DISKSAGE_TEST_RECLAIM_ROOT="$fixture_root/sdk-root" \
DISKSAGE_TEST_RECLAIM_MOUNT="/virtual-fixture-volume" \
GITHUB_STEP_SUMMARY="$summary_file" \
PATH="$fake_bin:$PATH" \
  bash "$helper" "$fixture_root/sdk-root"

test ! -e "$fixture_root/sdk-root"
reclaimed_bytes="$(sed -n 's/^runner_reclaimed_bytes=//p' "$summary_file")"
if ! [[ "$reclaimed_bytes" =~ ^[1-9][0-9]*$ ]]; then
  echo "expected positive runner_reclaimed_bytes, got '${reclaimed_bytes:-missing}'" >&2
  exit 1
fi
reclaim_filesystem="$(sed -n 's/^runner_reclaim_filesystem=//p' "$summary_file")"
if [[ "$reclaim_filesystem" != "/virtual-fixture-volume" ]]; then
  echo "expected fixture filesystem evidence, got '${reclaim_filesystem:-missing}'" >&2
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

echo "runner reclaim filesystem-bound evidence regression passed"
