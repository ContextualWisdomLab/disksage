import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const source = readFileSync('src-tauri/src/cargo_target_reclaim.rs', 'utf8');

function unixDetachBlockStart(): number {
  return source.indexOf('#[cfg(unix)]\nstruct DetachedTargetDir');
}

function windowsDetachBlockStart(): number {
  return source.indexOf('#[cfg(windows)]\nstruct DetachedTargetDir');
}

describe('Cargo target Unix retained-root cleanup ownership', () => {
  it('does not retain the obsolete Unix pathname quarantine/rollback implementation', () => {
    const unixDetach = unixDetachBlockStart();
    const windowsDetach = windowsDetachBlockStart();

    expect(
      windowsDetach,
      'Windows still owns its handle-bound detach compatibility path until HANDLE-relative descendant cleanup replaces it',
    ).toBeGreaterThanOrEqual(0);
    expect(
      unixDetach,
      'Unix cleanup is retained-descriptor based; the old pathname quarantine/rollback owner must not remain as dead destructive code',
    ).toBe(-1);
  });

  it('keeps the mutation-capable non-Linux Unix path descriptor-relative', () => {
    const flowStart = source.indexOf('fn clean_cargo_target_with_active_use_and_opened_hook');
    const ledgerStart = source.indexOf('/// Buyer-visible reclaim credit', flowStart);
    expect(flowStart).toBeGreaterThanOrEqual(0);
    expect(ledgerStart).toBeGreaterThan(flowStart);

    const flow = source.slice(flowStart, ledgerStart);
    expect(flow).toContain('crate::unix_capability_cleanup::measure_allocated_bytes(&opened_target.file)');
    expect(flow).toContain('crate::unix_capability_cleanup::remove_contents(&opened_target.file)');
    expect(flow).not.toContain('detach_verified_target_dir(&target_dir, opened_target)');
  });
});
