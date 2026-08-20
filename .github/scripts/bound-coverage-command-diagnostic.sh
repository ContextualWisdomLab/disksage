#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  printf 'usage: %s <raw-log> <bounded-log>\n' "$0" >&2
  exit 64
fi

raw_log="$1"
bounded_log="$2"
max_total_bytes=32768
edge_bytes=16000
max_line_bytes=2048
line_bounded_log="${bounded_log}.line-bounded.$$"

cleanup() {
  rm -f "$line_bounded_log"
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
  head -c "$edge_bytes" "$line_bounded_log" > "$bounded_log"
  printf '\n--- bounded diagnostic tail ---\n' >> "$bounded_log"
  tail -c "$edge_bytes" "$line_bounded_log" >> "$bounded_log"
fi
