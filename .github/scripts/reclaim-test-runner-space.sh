#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -lt 1 ]]; then
  echo "usage: $0 ROOT [ROOT ...]" >&2
  exit 64
fi

summary_file="${GITHUB_STEP_SUMMARY:-/dev/stdout}"
test_root="${DISKSAGE_RECLAIM_TEST_ROOT:-}"
if [[ -n "$test_root" ]]; then
  test_root="$(realpath -m -- "$test_root")"
fi

is_allowed_root() {
  local root="$1"
  case "$root" in
    /usr/local/lib/android|/usr/share/dotnet|/opt/ghc)
      return 0
      ;;
  esac

  if [[ -n "$test_root" && "$root" == "$test_root"/* ]]; then
    return 0
  fi
  return 1
}

roots=()
existing_roots=0
for requested_root in "$@"; do
  if [[ "$requested_root" != /* ]]; then
    echo "reclaim root must be absolute: $requested_root" >&2
    exit 65
  fi
  if [[ -L "$requested_root" ]]; then
    echo "reclaim root must not be a symlink: $requested_root" >&2
    exit 65
  fi

  canonical_root="$(realpath -m -- "$requested_root")"
  if ! is_allowed_root "$canonical_root"; then
    echo "reclaim root is outside the fixed runner allowlist: $canonical_root" >&2
    exit 65
  fi
  roots+=("$canonical_root")

  if [[ -e "$canonical_root" ]]; then
    if [[ ! -d "$canonical_root" ]]; then
      echo "reclaim root is not a directory: $canonical_root" >&2
      exit 65
    fi
    existing_roots=$((existing_roots + 1))
  fi
done

if [[ "$existing_roots" -eq 0 ]]; then
  echo "runner reclaim has no existing allowlisted root" >&2
  exit 67
fi

available_before="$(df --output=avail -B1 / | tail -1 | tr -d ' ')"
if ! [[ "$available_before" =~ ^[0-9]+$ ]]; then
  echo "could not measure runner availability before reclaim" >&2
  exit 68
fi

for root in "${roots[@]}"; do
  [[ -e "$root" ]] || continue
  parent="$(dirname -- "$root")"
  if [[ -w "$parent" ]]; then
    rm -rf -- "$root"
  else
    sudo rm -rf -- "$root"
  fi
  if [[ -e "$root" || -L "$root" ]]; then
    echo "runner reclaim did not remove exact root: $root" >&2
    exit 69
  fi
done

available_after="$(df --output=avail -B1 / | tail -1 | tr -d ' ')"
if ! [[ "$available_after" =~ ^[0-9]+$ ]]; then
  echo "could not measure runner availability after reclaim" >&2
  exit 68
fi
if [[ "$available_after" -le "$available_before" ]]; then
  echo "runner reclaim did not prove positive free-space recovery" >&2
  exit 71
fi
reclaimed_bytes=$((available_after - available_before))

{
  echo "runner_available_bytes_before=$available_before"
  echo "runner_available_bytes_after=$available_after"
  echo "runner_reclaimed_bytes=$reclaimed_bytes"
} >> "$summary_file"
