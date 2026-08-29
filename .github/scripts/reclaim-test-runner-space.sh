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

allocated_bytes() {
  local root="$1"
  timeout 30s du --summarize --block-size=1 -- "$root" | awk '{print $1}'
}

roots=()
before_allocated_bytes=0
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
    root_bytes="$(allocated_bytes "$canonical_root")"
    if ! [[ "$root_bytes" =~ ^[0-9]+$ ]]; then
      echo "could not measure reclaim root allocation: $canonical_root" >&2
      exit 66
    fi
    before_allocated_bytes=$((before_allocated_bytes + root_bytes))
    existing_roots=$((existing_roots + 1))
  fi
done

if [[ "$existing_roots" -eq 0 || "$before_allocated_bytes" -le 0 ]]; then
  echo "runner reclaim has no positive allocated-byte evidence" >&2
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

after_allocated_bytes=0
for root in "${roots[@]}"; do
  if [[ -e "$root" ]]; then
    root_bytes="$(allocated_bytes "$root")"
    after_allocated_bytes=$((after_allocated_bytes + root_bytes))
  fi
done

if [[ "$after_allocated_bytes" -gt "$before_allocated_bytes" ]]; then
  echo "runner reclaim allocation increased unexpectedly" >&2
  exit 70
fi
reclaimed_bytes=$((before_allocated_bytes - after_allocated_bytes))
if [[ "$reclaimed_bytes" -le 0 ]]; then
  echo "runner reclaim did not prove positive removed allocation" >&2
  exit 71
fi

available_after="$(df --output=avail -B1 / | tail -1 | tr -d ' ')"
if ! [[ "$available_after" =~ ^[0-9]+$ ]]; then
  echo "could not measure runner availability after reclaim" >&2
  exit 68
fi

{
  echo "runner_available_bytes_before=$available_before"
  echo "runner_available_bytes_after=$available_after"
  echo "runner_allocated_bytes_before=$before_allocated_bytes"
  echo "runner_allocated_bytes_after=$after_allocated_bytes"
  echo "runner_reclaimed_bytes=$reclaimed_bytes"
} >> "$summary_file"
