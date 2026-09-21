#!/usr/bin/env bash
# Guarded operational `cargo clean` — measurement scope == deletion scope.
# Do NOT use bare `cargo clean --manifest-path` alone (CARGO_TARGET_DIR /
# workspace / .cargo/config build.target-dir can redirect deletion).
#
# Fail-closed active-use: missing/failed lsof must NEVER proceed (including
# exit 127). Only documented empty no-match (exit 1, empty stdout+stderr) or
# exit 0 with zero holder lines allows clean. Any open holder refuses.
set -euo pipefail

USAGE='Usage: guarded-cargo-target-clean.sh --project-dir ABS_PATH [--target-dir ABS_PATH]

Resolves canonical project + target, requires target to be a strict child of
project (symlink escapes rejected), requires target ownership by the current
user, refuses when lsof is missing/failed or reports any open holder, then
runs: cargo clean --target-dir <approved> --manifest-path <project>/Cargo.toml
'

PROJECT_DIR=""
TARGET_DIR=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) printf '%s' "$USAGE"; exit 0 ;;
    --project-dir) PROJECT_DIR="${2:-}"; shift 2 ;;
    --target-dir) TARGET_DIR="${2:-}"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; printf '%s' "$USAGE" >&2; exit 2 ;;
  esac
done

[[ -n "$PROJECT_DIR" ]] || { echo "--project-dir required" >&2; exit 2; }
[[ "$PROJECT_DIR" = /* ]] || { echo "project-dir must be absolute" >&2; exit 2; }

CARGO="${CARGO_BIN:-$HOME/.cargo/bin/cargo}"
[[ -x "$CARGO" ]] || { echo "cargo unavailable: $CARGO" >&2; exit 2; }

PROJECT_REAL="$(cd "$PROJECT_DIR" && pwd -P)"
MANIFEST="$PROJECT_REAL/Cargo.toml"
[[ -f "$MANIFEST" ]] || { echo "Cargo.toml missing under $PROJECT_REAL" >&2; exit 2; }

if [[ -z "$TARGET_DIR" ]]; then
  TARGET_DIR="$PROJECT_REAL/target"
fi
[[ "$TARGET_DIR" = /* ]] || { echo "target-dir must be absolute" >&2; exit 2; }

if [[ -e "$TARGET_DIR" ]]; then
  TARGET_REAL="$(cd "$TARGET_DIR" && pwd -P)"
else
  PARENT_REAL="$(cd "$(dirname "$TARGET_DIR")" && pwd -P)"
  TARGET_REAL="$PARENT_REAL/$(basename "$TARGET_DIR")"
fi

# Component containment via python (avoid string-prefix false positives)
python3 - "$PROJECT_REAL" "$TARGET_REAL" <<'PY'
import pathlib, sys
root = pathlib.Path(sys.argv[1]).resolve()
cand = pathlib.Path(sys.argv[2]).resolve()
root_parts = root.parts
cand_parts = cand.parts
ok = len(cand_parts) > len(root_parts) and cand_parts[: len(root_parts)] == root_parts
if not ok:
    sys.stderr.write(f"target outside project (canonical): {cand} not under {root}\n")
    sys.exit(3)
PY

refuse_active_use() {
  local reason="$1"
  echo "refuse: $reason" >&2
  exit 5
}

# Owner gate: never clean a target we do not own (shared/other-user trees).
if [[ -e "$TARGET_REAL" ]]; then
  SELF_UID="$(id -u)"
  if stat --version >/dev/null 2>&1; then
    OWNER_UID="$(stat -c '%u' "$TARGET_REAL")"
  else
    OWNER_UID="$(stat -f '%u' "$TARGET_REAL")"
  fi
  if [[ "$OWNER_UID" != "$SELF_UID" ]]; then
    refuse_active_use "target-owner-mismatch owner_uid=$OWNER_UID self_uid=$SELF_UID path=$TARGET_REAL"
  fi
fi

# Active-holder probe — must not use `lsof | grep` under pipefail: lsof 127 / probe
# failure previously collapsed into "no match" and WOULD_PROCEED.
if [[ -d "$TARGET_REAL" ]]; then
  LSOF_BIN="${LSOF_BIN:-}"
  if [[ -z "$LSOF_BIN" ]]; then
    LSOF_BIN="$(command -v lsof || true)"
  fi
  if [[ -z "$LSOF_BIN" || ! -x "$LSOF_BIN" ]]; then
    refuse_active_use "lsof-unavailable (fail-closed; refusing clean of $TARGET_REAL)"
  fi

  LSOF_OUT="$(mktemp)"
  LSOF_ERR="$(mktemp)"
  cleanup_lsof_tmp() { rm -f "$LSOF_OUT" "$LSOF_ERR"; }
  trap cleanup_lsof_tmp EXIT

  set +e
  "$LSOF_BIN" +D "$TARGET_REAL" >"$LSOF_OUT" 2>"$LSOF_ERR"
  LSOF_EC=$?
  set -e

  # macOS may return exit 1 WITH holder lines (not only the documented empty
  # no-match). Any stdout ⇒ refuse. Any stderr ⇒ incomplete inspection (including
  # exit 0 + empty stdout + warning stderr) ⇒ refuse before accepting status.
  if [[ -s "$LSOF_OUT" ]]; then
    refuse_active_use "active-holders-present path=$TARGET_REAL (lsof_exit=$LSOF_EC)"
  elif [[ -s "$LSOF_ERR" ]]; then
    refuse_active_use "lsof-stderr-nonempty path=$TARGET_REAL (lsof_exit=$LSOF_EC)"
  elif [[ "$LSOF_EC" -eq 0 || "$LSOF_EC" -eq 1 ]]; then
    : # exit 0/1 with empty stdout and stderr only
  else
    refuse_active_use "lsof-exit-status:$LSOF_EC path=$TARGET_REAL (fail-closed)"
  fi
fi

BEFORE_KIB=0
if [[ -d "$TARGET_REAL" ]]; then
  BEFORE_KIB="$(du -sk "$TARGET_REAL" | awk '{print $1}')"
fi
DF_BEFORE="$(df -k /System/Volumes/Data 2>/dev/null | tail -1 || df -k / | tail -1)"

EC=0
"$CARGO" clean --manifest-path "$MANIFEST" --target-dir "$TARGET_REAL" || EC=$?

AFTER_KIB=0
if [[ -d "$TARGET_REAL" ]]; then
  AFTER_KIB="$(du -sk "$TARGET_REAL" | awk '{print $1}')"
fi
DF_AFTER="$(df -k /System/Volumes/Data 2>/dev/null | tail -1 || df -k / | tail -1)"

python3 - "$CARGO" "$PROJECT_REAL" "$TARGET_REAL" "$EC" \
  "$BEFORE_KIB" "$AFTER_KIB" "$DF_BEFORE" "$DF_AFTER" <<'PY'
import json
import sys

cargo, project_dir, target_dir, exit_code, before, after, df_before, df_after = sys.argv[1:]
before_i = int(before)
after_i = int(after)
print(json.dumps({
  "cargo": cargo,
  "project_dir": project_dir,
  "target_dir": target_dir,
  "exit_code": int(exit_code),
  "du_kib_before": before_i,
  "du_kib_after": after_i,
  "observed_reduction_kib": max(0, before_i - after_i),
  "df_before": df_before,
  "df_after": df_after,
}, indent=2))
PY
exit "$EC"
