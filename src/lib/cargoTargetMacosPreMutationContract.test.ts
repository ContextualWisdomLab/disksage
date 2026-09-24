import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const source = readFileSync('src-tauri/src/cargo_target_reclaim.rs', 'utf8');

function ownerFlow(): string {
  const start = source.indexOf('fn clean_cargo_target_with_active_use_and_opened_hook');
  const end = source.indexOf('/// Buyer-visible reclaim credit', start);
  expect(start, 'cargo target owner flow must remain present').toBeGreaterThanOrEqual(0);
  expect(end, 'ledger boundary must remain after the cargo target owner flow').toBeGreaterThan(start);
  return source.slice(start, end);
}

describe('macOS cargo-target pre-mutation final-object authority', () => {
  it('fails closed after exact holder authorization and before measurement or mutation', () => {
    const flow = ownerFlow();
    const exactHolder = flow.indexOf(
      'unix_holder_authority::ensure_opened_target_has_no_active_holders',
    );
    const macosCutoff = flow.indexOf('cargo-target-macos-final-object-authority-unproven');
    const measurement = flow.indexOf('unix_capability_cleanup::measure_allocated_bytes');
    const mutation = flow.indexOf('unix_capability_cleanup::remove_contents');

    expect(exactHolder, 'exact-object holder authorization must remain present').toBeGreaterThanOrEqual(0);
    expect(
      macosCutoff,
      'macOS must refuse before destructive traversal until native final-object mutation authority is proven under #170',
    ).toBeGreaterThan(exactHolder);
    expect(
      measurement,
      'non-Linux Unix measurement remains explicit for platforms with proven mutation authority',
    ).toBeGreaterThan(macosCutoff);
    expect(
      mutation,
      'non-Linux Unix mutation remains explicit for platforms with proven mutation authority',
    ).toBeGreaterThan(macosCutoff);
  });

  it('keeps the macOS refusal platform-scoped instead of disabling reusable Unix cleanup', () => {
    const flow = ownerFlow();
    const macosCutoff = flow.indexOf('cargo-target-macos-final-object-authority-unproven');
    expect(macosCutoff).toBeGreaterThanOrEqual(0);

    const guardWindow = flow.slice(Math.max(0, macosCutoff - 220), macosCutoff);
    expect(guardWindow).toContain('#[cfg(target_os = "macos")]');
    expect(flow).toContain('#[cfg(all(unix, not(target_os = "linux")))]');
    expect(flow).toContain('crate::unix_capability_cleanup::remove_contents(&opened_target.file)');
  });
});
