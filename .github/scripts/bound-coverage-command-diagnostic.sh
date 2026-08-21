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
error_focus_bytes=8000
other_focus_bytes=4000
max_line_bytes=2048
line_bounded_log="${bounded_log}.line-bounded.$$"
error_focus_log="${bounded_log}.error-focus.$$"
other_focus_log="${bounded_log}.other-focus.$$"

cleanup() {
  rm -f "$line_bounded_log" "$error_focus_log" "$other_focus_log"
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
  # Put actionable Rust/Cargo errors ahead of warning noise. An edge-only or warning-first focus can
  # otherwise omit the first compiler error when a build emits many warnings before it fails.
  LC_ALL=C awk '
    /^(error(\[[^]]+\])?:|fatal:|Caused by:)/ {
      print
      in_error = 1
      next
    }
    in_error && /^[[:space:]]*(--> |[0-9]+[[:space:]]*\||\|[[:space:]]|= (note|help):|(note|help):)/ {
      print
      next
    }
    { in_error = 0 }
  ' "$line_bounded_log" > "$error_focus_log"

  LC_ALL=C grep -E \
    '^warning(\[[^]]+\])?:|^[[:space:]]*= (note|help):|^[[:space:]]*(note|help):' \
    "$line_bounded_log" > "$other_focus_log" || true

  head -c "$edge_bytes" "$line_bounded_log" > "$bounded_log"
  if [[ -s "$error_focus_log" ]]; then
    printf '\n--- focused compiler errors ---\n' >> "$bounded_log"
    head -c "$error_focus_bytes" "$error_focus_log" >> "$bounded_log"
  fi
  if [[ -s "$other_focus_log" ]]; then
    printf '\n--- focused compiler warnings and notes ---\n' >> "$bounded_log"
    head -c "$other_focus_bytes" "$other_focus_log" >> "$bounded_log"
  fi
  printf '\n--- bounded diagnostic tail ---\n' >> "$bounded_log"
  tail -c "$edge_bytes" "$line_bounded_log" >> "$bounded_log"
fi
