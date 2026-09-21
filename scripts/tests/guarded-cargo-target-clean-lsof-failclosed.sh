#!/usr/bin/env bash
# Isolated regression: lsof exit 127 must NOT WOULD_PROCEED under set -euo pipefail.
# Uses only temp fixtures — never touches real project targets / AC lanes / venvs.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCRIPT="$ROOT/scripts/guarded-cargo-target-clean.sh"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/disksage-guarded-lsof-XXXXXX")"
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

mkdir -p "$WORK/bin" "$WORK/proj/target"
printf '%s\n' '[package]
name = "t"
version = "0.1.0"
edition = "2021"
' >"$WORK/proj/Cargo.toml"
printf 'x' >"$WORK/proj/target/keep-me"

# Stub cargo that would delete if invoked — must never run on lsof-127.
cat >"$WORK/bin/cargo" <<'EOS'
#!/bin/sh
echo CARGO_RAN > "$(dirname "$0")/../cargo-ran"
exit 0
EOS
chmod +x "$WORK/bin/cargo"

# Stub lsof: exit 127 (command-not-found class)
cat >"$WORK/bin/lsof" <<'EOS'
#!/bin/sh
exit 127
EOS
chmod +x "$WORK/bin/lsof"

set +e
LSOF_BIN="$WORK/bin/lsof" CARGO_BIN="$WORK/bin/cargo" \
  "$SCRIPT" --project-dir "$WORK/proj" --target-dir "$WORK/proj/target" \
  >"$WORK/out.txt" 2>"$WORK/err.txt"
EC=$?
set -e

if [[ "$EC" -eq 0 ]]; then
  echo "FAIL: guarded script exited 0 on lsof 127 (WOULD_PROCEED)" >&2
  exit 1
fi
if [[ -f "$WORK/cargo-ran" ]]; then
  echo "FAIL: cargo ran despite lsof 127" >&2
  exit 1
fi
if [[ ! -f "$WORK/proj/target/keep-me" ]]; then
  echo "FAIL: target contents deleted despite lsof 127" >&2
  exit 1
fi
grep -q 'lsof-exit-status:127\|lsof-unavailable' "$WORK/err.txt" \
  || { echo "FAIL: expected lsof fail-closed reason in stderr"; cat "$WORK/err.txt" >&2; exit 1; }

# Holder present must also refuse
cat >"$WORK/bin/lsof" <<'EOS'
#!/bin/sh
echo "COMMAND PID USER"
echo "nativebin 999 me"
exit 0
EOS
chmod +x "$WORK/bin/lsof"
set +e
LSOF_BIN="$WORK/bin/lsof" CARGO_BIN="$WORK/bin/cargo" \
  "$SCRIPT" --project-dir "$WORK/proj" --target-dir "$WORK/proj/target" \
  >"$WORK/out2.txt" 2>"$WORK/err2.txt"
EC2=$?
set -e
[[ "$EC2" -ne 0 ]] || { echo "FAIL: holders present exited 0"; exit 1; }
[[ ! -f "$WORK/cargo-ran" ]] || { echo "FAIL: cargo ran with holders"; exit 1; }
[[ -f "$WORK/proj/target/keep-me" ]] || { echo "FAIL: deleted with holders"; exit 1; }

echo "PASS guarded-cargo-target-clean lsof fail-closed regressions"
