#!/usr/bin/env bash
# Compatibility entry point only. Deletion safety, path admission, active-use
# evidence, measurement and Cargo execution are owned by the Rust CLI/domain.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CLI="${DISKSAGE_CARGO_TARGET_CLEAN_BIN:-}"

if [[ -z "$CLI" ]]; then
  CLI="$(command -v disksage-cargo-target-clean || true)"
fi
if [[ -z "$CLI" && -x "$ROOT/src-tauri/target/release/disksage-cargo-target-clean" ]]; then
  CLI="$ROOT/src-tauri/target/release/disksage-cargo-target-clean"
fi
if [[ -z "$CLI" || ! -x "$CLI" ]]; then
  printf 'disksage-cargo-target-clean unavailable; build/install the Rust owner CLI or set DISKSAGE_CARGO_TARGET_CLEAN_BIN.\n' >&2
  exit 127
fi

exec "$CLI" "$@"
