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

describe('Unix post-open holder authorization ordering', () => {
  it('binds mutation authorization to the reviewed root after the post-open seam and before detach', () => {
    const flow = ownerFlow();
    const preflight = flow.indexOf('active_use(&target_dir)?;');
    const opened = flow.indexOf('open_verified_target_dir');
    const afterOpen = flow.indexOf('after_open(&target_dir)?;');
    const exactHolder = flow.indexOf(
      'unix_holder_authority::ensure_opened_target_has_no_active_holders(&opened_target.file)',
    );
    const detach = flow.indexOf('detach_verified_target_dir');

    expect(preflight, 'pathname preflight remains an early refusal only').toBeGreaterThanOrEqual(0);
    expect(opened, 'the target must be opened and identity-bound before exact holder authorization').toBeGreaterThan(preflight);
    expect(afterOpen, 'the deterministic post-open substitution seam must remain before authorization').toBeGreaterThan(opened);
    expect(
      exactHolder,
      'Unix mutation authorization must inspect holders of the retained reviewed File, not a later pathname',
    ).toBeGreaterThan(afterOpen);
    expect(
      detach,
      'the current holder-only tranche must authorize the reviewed object before any detach/mutation boundary',
    ).toBeGreaterThan(exactHolder);
  });

  it('keeps the exact-object holder call Unix-scoped', () => {
    const flow = ownerFlow();
    const exactHolder = flow.indexOf(
      'unix_holder_authority::ensure_opened_target_has_no_active_holders(&opened_target.file)',
    );
    expect(exactHolder).toBeGreaterThanOrEqual(0);
    const guardWindow = flow.slice(Math.max(0, exactHolder - 80), exactHolder);
    expect(guardWindow).toContain('#[cfg(unix)]');
  });
});
