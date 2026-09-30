import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const source = readFileSync('src-tauri/src/unix_holder_authority.rs', 'utf8');

describe('Unix holder evidence completeness', () => {
  it('does not treat a security-restricted lsof build as system-wide evidence', () => {
    expect(source).toContain('cargo-target-lsof-global-visibility-unavailable');
    expect(source).toMatch(/Anyone can list all files/);
    expect(source).toMatch(/geteuid|effective_uid/);
  });

  it('checks lsof visibility capability before using the global file listing for authorization', () => {
    expect(source).toMatch(/verify_lsof_global_visibility|verify_lsof_visibility/);
    const authorization = source.indexOf('ensure_opened_target_has_no_active_holders');
    expect(authorization).toBeGreaterThanOrEqual(0);
    const body = source.slice(authorization);
    const visibility = Math.min(
      ...['verify_lsof_global_visibility', 'verify_lsof_visibility']
        .map((symbol) => body.indexOf(symbol))
        .filter((index) => index >= 0),
    );
    const listing = body.indexOf('run_lsof');
    expect(visibility).toBeGreaterThanOrEqual(0);
    expect(listing).toBeGreaterThan(visibility);
  });

  it('keeps an owner-local regression for restricted non-root lsof builds', () => {
    expect(source).toContain('restricted_lsof_security_mode_fails_closed_for_non_root');
  });
});
