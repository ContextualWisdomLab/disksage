#!/usr/bin/env bash
# The compatibility shell must preserve argv/exit status while delegating all
# deletion decisions to the Rust owner CLI.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCRIPT="$ROOT/scripts/guarded-cargo-target-clean.sh"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/disksage-cargo-launcher-XXXXXX")"
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

mkdir -p "$WORK/project with space"
CLI="$WORK/disksage-cargo-target-clean"
LOG="$WORK/argv.bin"
cat >"$CLI" <<'EOS'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\0' "$@" >"$DELEGATE_LOG"
exit 23
EOS
chmod +x "$CLI"

set +e
DELEGATE_LOG="$LOG" DISKSAGE_CARGO_TARGET_CLEAN_BIN="$CLI" \
  "$SCRIPT" --project-dir "$WORK/project with space" >"$WORK/stdout" 2>"$WORK/stderr"
EC=$?
set -e

[[ "$EC" -eq 23 ]] || { echo "FAIL: launcher did not preserve owner CLI exit status ($EC)" >&2; exit 1; }
printf '%s\0' --project-dir "$WORK/project with space" >"$WORK/expected.bin"
cmp "$WORK/expected.bin" "$LOG" \
  || { echo "FAIL: launcher changed owner CLI argv" >&2; exit 1; }
[[ ! -s "$WORK/stdout" ]] || { echo "FAIL: launcher added stdout" >&2; exit 1; }
[[ ! -s "$WORK/stderr" ]] || { echo "FAIL: launcher added stderr" >&2; exit 1; }

echo "PASS guarded-cargo-target-clean delegates to Rust owner"
