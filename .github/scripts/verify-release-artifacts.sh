#!/usr/bin/env bash
set -euo pipefail

artifact_root="${1:-release-artifacts}"
run_attempt="${2:-}"

if [[ -z "$run_attempt" || ! "$run_attempt" =~ ^[1-9][0-9]*$ ]]; then
  printf 'run attempt must be a positive integer.\n' >&2
  exit 1
fi
if [[ ! -d "$artifact_root" ]]; then
  printf 'release artifact root is missing: %s\n' "$artifact_root" >&2
  exit 1
fi

require_exactly_one_path() {
  local path_pattern="$1" label="$2" count=0 matched_path=""
  while IFS= read -r -d '' matched_path; do count=$((count + 1)); done < <(find "$artifact_root" -type f -path "$path_pattern" -print0)
  if [[ $count -ne 1 ]]; then
    printf 'Expected exactly one %s, found %s.\n' "$label" "$count" >&2
    exit 1
  fi
}

require_exactly_one_file() {
  local file_name="$1" count=0 matched_path=""
  while IFS= read -r -d '' matched_path; do count=$((count + 1)); done < <(find "$artifact_root" -type f -name "$file_name" -print0)
  if [[ $count -ne 1 ]]; then
    printf 'Expected exactly one release artifact named %s, found %s.\n' "$file_name" "$count" >&2
    exit 1
  fi
}

expected_dirs=(
  "release-disksage-ubuntu-22.04-${run_attempt}"
  "release-disksage-windows-2022-${run_attempt}"
  "release-disksage-macos-latest-${run_attempt}"
)

mapfile -d '' top_level_entries < <(find "$artifact_root" -mindepth 1 -maxdepth 1 -print0 | sort -z)
if [[ ${#top_level_entries[@]} -ne ${#expected_dirs[@]} ]]; then
  printf 'Expected exactly three release artifact directories, found %s.\n' "${#top_level_entries[@]}" >&2
  exit 1
fi
for expected_dir in "${expected_dirs[@]}"; do
  if [[ ! -d "$artifact_root/$expected_dir" || -L "$artifact_root/$expected_dir" ]]; then
    printf 'Expected release artifact directory is missing or unsafe: %s\n' "$expected_dir" >&2
    exit 1
  fi
done

unexpected_entry="$(find "$artifact_root" -mindepth 1 ! -type d ! -type f -print -quit)"
if [[ -n "$unexpected_entry" ]]; then
  printf 'Unexpected release artifact entries: non-regular path %s is not publishable.\n' "$unexpected_entry" >&2
  exit 1
fi

require_exactly_one_path '*/bundle/deb/*.deb' 'Debian bundle'
require_exactly_one_path '*/bundle/appimage/*.AppImage' 'AppImage bundle'
require_exactly_one_path '*/bundle/msi/*.msi' 'Windows MSI bundle'
require_exactly_one_path '*/bundle/nsis/*.exe' 'Windows NSIS bundle'
require_exactly_one_path '*/bundle/dmg/*.dmg' 'macOS DMG bundle'

for required_name in \
  disksage-cloud-plan-linux-x86_64 \
  disksage-duplicate-audit-linux-x86_64 \
  disksage-cloud-plan-windows-x86_64.exe \
  disksage-duplicate-audit-windows-x86_64.exe \
  disksage-cloud-plan-macos-arm64 \
  disksage-duplicate-audit-macos-arm64 \
  disksage-cloud-local-eviction-batch-macos-arm64 \
  disksage-icloud-local-eviction-batch-macos-arm64 \
  disksage-cloud-local-inventory-macos-arm64 \
  disksage-onedrive-finder-verify-macos-arm64; do
  require_exactly_one_file "$required_name"
  require_exactly_one_file "$required_name.sha256"
done

checksum_files=()
checksum_file=""
while IFS= read -r -d '' checksum_file; do checksum_files+=("$checksum_file"); done < <(find "$artifact_root" -type f -name '*.sha256' -print0)
if [[ ${#checksum_files[@]} -ne 10 ]]; then
  printf 'Expected ten operational CLI checksum files, found %s.\n' "${#checksum_files[@]}" >&2
  exit 1
fi

for checksum_file in "${checksum_files[@]}"; do
  checksum_dir="$(dirname "$checksum_file")"
  checksum_name="$(basename "$checksum_file")"
  expected_asset_name="${checksum_name%.sha256}"
  checksum_line="" line="" line_count=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    line_count=$((line_count + 1))
    checksum_line="$line"
  done <"$checksum_file"
  if [[ $line_count -ne 1 ]]; then
    printf 'Checksum file %s must contain exactly one record.\n' "$checksum_name" >&2
    exit 1
  fi
  recorded_digest="" recorded_name="" extra_field=""
  read -r recorded_digest recorded_name extra_field <<<"$checksum_line"
  if [[ "$recorded_name" == \** ]]; then
    recorded_name="${recorded_name#\*}"
  fi
  if [[ ! "$recorded_digest" =~ ^[0-9a-fA-F]{64}$ ]] || [[ "$recorded_name" != "$expected_asset_name" ]] || [[ -n "$extra_field" ]]; then
    printf 'Checksum file %s must reference its adjacent operational CLI %s exactly once.\n' "$checksum_name" "$expected_asset_name" >&2
    exit 1
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$checksum_dir" && sha256sum --check "$checksum_name")
  elif command -v shasum >/dev/null 2>&1; then
    (cd "$checksum_dir" && shasum -a 256 --check "$checksum_name")
  else
    printf 'No SHA-256 checksum verifier is available.\n' >&2
    exit 1
  fi
done

regular_file_count=0
matched_path=""
while IFS= read -r -d '' matched_path; do regular_file_count=$((regular_file_count + 1)); done < <(find "$artifact_root" -type f -print0)
if [[ $regular_file_count -ne 25 ]]; then
  printf 'Unexpected release artifact entries: expected exactly 25 regular files, found %s.\n' "$regular_file_count" >&2
  exit 1
fi
