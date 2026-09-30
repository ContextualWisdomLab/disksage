#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -lt 1 ]]; then
  echo "usage: $0 ROOT [ROOT ...]" >&2
  exit 64
fi

if [[ "${GITHUB_ACTIONS:-}" != "true" || "${RUNNER_ENVIRONMENT:-}" != "github-hosted" ]]; then
  echo "runner reclaim requires a GitHub-hosted Actions runner" >&2
  exit 66
fi

summary_file="${GITHUB_STEP_SUMMARY:-/dev/stdout}"
test_root="${DISKSAGE_RECLAIM_TEST_ROOT:-}"
if [[ -n "$test_root" ]]; then
  if [[ "${DISKSAGE_RECLAIM_TEST_MODE:-}" != "true" || -z "${RUNNER_TEMP:-}" ]]; then
    echo "test reclaim root requires explicit runner-temp test authority" >&2
    exit 65
  fi
  test_root="$(realpath -m -- "$test_root")"
  runner_temp="$(realpath -m -- "$RUNNER_TEMP")"
  if [[ "$test_root" != "$runner_temp"/* ]]; then
    echo "test reclaim root must be below the GitHub runner temp directory" >&2
    exit 65
  fi
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

filesystem_state() {
  local path="$1"
  local line
  line="$(df --output=target,avail -B1 -- "$path" | tail -n 1)" || return 1
  local available="${line##* }"
  local mount_target="${line%"$available"}"
  mount_target="${mount_target#"${mount_target%%[![:space:]]*}"}"
  mount_target="${mount_target%"${mount_target##*[![:space:]]}"}"
  if [[ -z "$mount_target" || ! "$available" =~ ^[0-9]+$ ]]; then
    return 1
  fi
  printf '%s\t%s\n' "$mount_target" "$available"
}

roots=()
mount_targets=()
mount_available_before=()
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
    state="$(filesystem_state "$canonical_root")" || {
      echo "could not measure reclaim-root filesystem" >&2
      exit 68
    }
    mount_target="${state%%$'\t'*}"
    available_before="${state##*$'\t'}"

    already_recorded=false
    for known_mount in "${mount_targets[@]:-}"; do
      if [[ "$known_mount" == "$mount_target" ]]; then
        already_recorded=true
        break
      fi
    done
    if [[ "$already_recorded" == false ]]; then
      mount_targets+=("$mount_target")
      mount_available_before+=("$available_before")
    fi
    existing_roots=$((existing_roots + 1))
  fi
done

if [[ "$existing_roots" -eq 0 ]]; then
  echo "runner reclaim has no existing allowlisted root" >&2
  exit 67
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

reclaimed_bytes=0
summary_lines=()
for index in "${!mount_targets[@]}"; do
  mount_target="${mount_targets[$index]}"
  available_before="${mount_available_before[$index]}"
  state="$(filesystem_state "$mount_target")" || {
    echo "could not remeasure reclaim-root filesystem" >&2
    exit 68
  }
  measured_mount="${state%%$'\t'*}"
  available_after="${state##*$'\t'}"
  if [[ "$measured_mount" != "$mount_target" ]]; then
    echo "reclaim-root filesystem identity changed during cleanup" >&2
    exit 70
  fi
  if [[ "$available_after" -le "$available_before" ]]; then
    echo "runner reclaim did not prove positive free-space recovery" >&2
    exit 71
  fi
  filesystem_reclaimed=$((available_after - available_before))
  reclaimed_bytes=$((reclaimed_bytes + filesystem_reclaimed))
  summary_lines+=("runner_reclaim_filesystem=$mount_target")
  summary_lines+=("runner_available_bytes_before=$available_before")
  summary_lines+=("runner_available_bytes_after=$available_after")
done

if [[ "$reclaimed_bytes" -le 0 ]]; then
  echo "runner reclaim did not prove positive free-space recovery" >&2
  exit 71
fi

{
  printf '%s\n' "${summary_lines[@]}"
  echo "runner_reclaimed_bytes=$reclaimed_bytes"
} >> "$summary_file"
