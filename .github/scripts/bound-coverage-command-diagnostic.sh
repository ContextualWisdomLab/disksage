#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  printf 'usage: %s <raw-log> <bounded-log>\n' "$0" >&2
  exit 64
fi

raw_log="$1"
bounded_log="$2"
max_total_bytes=32768
edge_bytes=9000
focus_bytes=12000
max_line_bytes=2048
line_bounded_log="${bounded_log}.line-bounded.$$"
diagnostic_focus_log="${bounded_log}.diagnostic-focus.$$"

cleanup() {
  rm -f "$line_bounded_log" "$diagnostic_focus_log"
}
trap cleanup EXIT

LC_ALL=C awk -v max_bytes="$max_line_bytes" '{
  if (length($0) > max_bytes) {
    print substr($0, 1, max_bytes) " ... [line truncated]"
  } else {
    print
  }
}' "$raw_log" > "$line_bounded_log"

diagnostic_bytes="$(wc -c < "$line_bounded_log" | tr -d ' ')"
if (( diagnostic_bytes <= max_total_bytes )); then
  cp "$line_bounded_log" "$bounded_log"
else
  # Preserve Rust/Cargo diagnostic anchors even when they land in the middle of a noisy log.
  # Edge-only truncation can otherwise remove the first actionable compiler error entirely.
  LC_ALL=C grep -E \
    '^(error|warning)(\[[^]]+\])?:|^fatal:|^Caused by:|^[[:space:]]*--> |^[[:space:]]*[0-9]+[[:space:]]*\||^[[:space:]]*\|[[:space:]]|^[[:space:]]*= (note|help):|^[[:space:]]*(note|help):' \
    "$line_bounded_log" > "$diagnostic_focus_log" || true

  head -c "$edge_bytes" "$line_bounded_log" > "$bounded_log"
  if [[ -s "$diagnostic_focus_log" ]]; then
    printf '\n--- focused compiler diagnostics ---\n' >> "$bounded_log"
    head -c "$focus_bytes" "$diagnostic_focus_log" >> "$bounded_log"
  fi
  printf '\n--- bounded diagnostic tail ---\n' >> "$bounded_log"
  tail -c "$edge_bytes" "$line_bounded_log" >> "$bounded_log"
fi
