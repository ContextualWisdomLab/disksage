import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const source = readFileSync('src-tauri/src/cache_cleanup.rs', 'utf8');

function sourceSlice(startMarker: string, endMarker: string): string {
  const start = source.indexOf(startMarker);
  expect(start).toBeGreaterThanOrEqual(0);
  const end = source.indexOf(endMarker, start + startMarker.length);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe('uv cache cleanup owner boundary', () => {
  it('does not classify uv private buckets by basename', () => {
    // archive-v0 is a private uv cache implementation detail, not DiskSage domain truth.
    // In particular, an unrelated catalog child with the same basename must not acquire
    // different deletion semantics merely because its path happens to end in archive-v0.
    expect(source).not.toContain('UV_TOOL_ARCHIVE_DIR');
    expect(source).not.toContain('cache-target-protected-uv-tool-archive');

    const cleanup = sourceSlice(
      'pub(crate) fn clean_cache_contents_inner(',
      'pub(crate) fn clean_regenerable_caches_inner(',
    );
    expect(cleanup).not.toMatch(/file_name\(\)[\s\S]*archive-v0/);
  });

  it('keeps generic Trash cleanup out of the uv cache until the native uv owner is used', () => {
    const automaticIds = sourceSlice(
      'pub const AUTO_REGENERABLE_CACHE_IDS',
      'const PROVEN_CACHE_TRASH_NAMES',
    );
    expect(automaticIds).not.toContain('"uv-cache"');

    const cleanup = sourceSlice(
      'pub(crate) fn clean_cache_contents_inner(',
      'pub(crate) fn clean_regenerable_caches_inner(',
    );
    const ownerGate = cleanup.indexOf('is_uv_cache_root(bases, dir)');
    const snapshotRefresh = cleanup.indexOf('rules::cache_targets(dir)?');
    expect(ownerGate).toBeGreaterThanOrEqual(0);
    expect(snapshotRefresh).toBeGreaterThan(ownerGate);
    expect(cleanup).toContain('uv-cache-native-owner-required');

    const listTargets = sourceSlice(
      'pub fn list_cache_targets(',
      '/// Move only the reviewed cache children to the OS Trash',
    );
    const previewOwnerGate = listTargets.indexOf('is_uv_cache_root(&bases, Path::new(&dir))');
    const previewEnumeration = listTargets.indexOf('rules::cache_targets(Path::new(&dir))');
    expect(previewOwnerGate).toBeGreaterThanOrEqual(0);
    expect(previewEnumeration).toBeGreaterThan(previewOwnerGate);
    expect(listTargets).toContain('uv-cache-native-owner-required');
  });
});
