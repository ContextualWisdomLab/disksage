#!/usr/bin/env bash
# Guarded operational `cargo clean` — measurement scope == deletion scope.
# Do NOT use bare `cargo clean --manifest-path` alone (CARGO_TARGET_DIR /
# workspace / .cargo/config build.target-dir can redirect deletion).
set -euo pipefail

USAGE='Usage: guarded-cargo-target-clean.sh --project-dir ABS_PATH [--target-dir ABS_PATH]

Resolves canonical project + target, requires target to be a strict child of
project (symlink escapes rejected), refuses if cargo/rustc hold the target,
then runs: cargo clean --target-dir <approved> --manifest-path <project>/Cargo.toml
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

# Refuse shared/busy targets
if [[ -d "$TARGET_REAL" ]]; then
  if lsof +D "$TARGET_REAL" 2>/dev/null | grep -E 'cargo|rustc' >/dev/null; then
    echo "refuse: cargo/rustc still using $TARGET_REAL" >&2
    exit 4
  fi
fi

BEFORE_KIB=0
if [[ -d "$TARGET_REAL" ]]; then
  BEFORE_KIB="$(du -sk "$TARGET_REAL" | awk '{print $1}')"
fi
DF_BEFORE="$(df -k /System/Volumes/Data 2>/dev/null | tail -1 || df -k / | tail -1)"

"$CARGO" clean --manifest-path "$MANIFEST" --target-dir "$TARGET_REAL"
EC=$?

AFTER_KIB=0
if [[ -d "$TARGET_REAL" ]]; then
  AFTER_KIB="$(du -sk "$TARGET_REAL" | awk '{print $1}')"
fi
DF_AFTER="$(df -k /System/Volumes/Data 2>/dev/null | tail -1 || df -k / | tail -1)"

python3 - <<PY
import json
print(json.dumps({
  "cargo": "$CARGO",
  "project_dir": "$PROJECT_REAL",
  "target_dir": "$TARGET_REAL",
  "exit_code": $EC,
  "du_kib_before": $BEFORE_KIB,
  "du_kib_after": $AFTER_KIB,
  "observed_reduction_kib": max(0, $BEFORE_KIB - $AFTER_KIB),
  "df_before": "$DF_BEFORE",
  "df_after": "$DF_AFTER",
}, indent=2))
PY
exit "$EC"
