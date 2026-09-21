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

# Holder present must also refuse (exit 0 + stdout)
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

# macOS-style: exit 1 WITH holder stdout (Python/non-cargo) must refuse — not WOULD_PROCEED
cat >"$WORK/bin/lsof" <<'EOS'
#!/bin/sh
echo "COMMAND     PID       USER   FD   TYPE DEVICE SIZE/OFF      NODE NAME"
echo "python3.1 21407 seonghobae    3u   REG   1,16       23 1 /tmp/x/target/synthetic-artifact"
exit 1
EOS
chmod +x "$WORK/bin/lsof"
set +e
LSOF_BIN="$WORK/bin/lsof" CARGO_BIN="$WORK/bin/cargo" \
  "$SCRIPT" --project-dir "$WORK/proj" --target-dir "$WORK/proj/target" \
  >"$WORK/out3.txt" 2>"$WORK/err3.txt"
EC3=$?
set -e
[[ "$EC3" -ne 0 ]] || { echo "FAIL: exit1+holder-stdout WOULD_PROCEED"; exit 1; }
grep -q 'active-holders-present' "$WORK/err3.txt" \
  || { echo "FAIL: expected active-holders-present for exit1+stdout"; cat "$WORK/err3.txt" >&2; exit 1; }
[[ ! -f "$WORK/cargo-ran" ]] || { echo "FAIL: cargo ran on exit1+holder-stdout"; exit 1; }
[[ -f "$WORK/proj/target/keep-me" ]] || { echo "FAIL: deleted on exit1+holder-stdout"; exit 1; }

# report-a118 warning_zero: exit 0 + empty stdout + warning stderr must NOT call cargo
cat >"$WORK/bin/lsof" <<'EOS'
#!/bin/sh
echo "lsof: WARNING: can't stat() fuse.portal file system /run/user/0/doc" >&2
exit 0
EOS
chmod +x "$WORK/bin/lsof"
set +e
LSOF_BIN="$WORK/bin/lsof" CARGO_BIN="$WORK/bin/cargo" \
  "$SCRIPT" --project-dir "$WORK/proj" --target-dir "$WORK/proj/target" \
  >"$WORK/out4.txt" 2>"$WORK/err4.txt"
EC4=$?
set -e
[[ "$EC4" -ne 0 ]] || { echo "FAIL: exit0+warning-stderr WOULD_PROCEED"; exit 1; }
grep -q 'lsof-stderr-nonempty' "$WORK/err4.txt" \
  || { echo "FAIL: expected lsof-stderr-nonempty"; cat "$WORK/err4.txt" >&2; exit 1; }
[[ ! -f "$WORK/cargo-ran" ]] || { echo "FAIL: cargo ran on exit0+warning-stderr"; exit 1; }
[[ -f "$WORK/proj/target/keep-me" ]] || { echo "FAIL: deleted on exit0+warning-stderr"; exit 1; }

echo "PASS guarded-cargo-target-clean lsof fail-closed regressions"
