import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');

describe('private staging creation rollback contract', () => {
  it('does not use infallible Drop as the authority for a fallible rollback', () => {
    const guardStart = safetySource.indexOf('struct StagingCreationGuard');
    const parentOpenStart = safetySource.indexOf('fn open_staging_parent(', guardStart);

    expect(guardStart, 'StagingCreationGuard must exist').toBeGreaterThanOrEqual(0);
    expect(parentOpenStart, 'open_staging_parent must follow the guard').toBeGreaterThan(guardStart);

    const guardBlock = safetySource.slice(guardStart, parentOpenStart);
    expect(
      guardBlock,
      'rollback must be an explicit fallible operation so cleanup failure can be handled before return',
    ).toMatch(/fn\s+(rollback|abort)\s*\([^)]*\)\s*->\s*(std::io::)?Result\s*</);
    expect(
      guardBlock,
      'a fallible staging rollback must not be hidden in Drop',
    ).not.toContain('impl Drop for StagingCreationGuard');
    expect(
      guardBlock,
      'non-Unix staging cleanup failures must not be discarded',
    ).not.toContain('let _ = std::fs::remove_dir');
  });

  it('captures staging identity before the deterministic post-create race is injected', () => {
    const createStart = safetySource.indexOf('fn create_private_staging_dir(');
    const restoreStart = safetySource.indexOf('fn restore_staged_if_source_absent(', createStart);

    expect(createStart, 'create_private_staging_dir must exist').toBeGreaterThanOrEqual(0);
    expect(restoreStart).toBeGreaterThan(createStart);

    const body = safetySource.slice(createStart, restoreStart);
    const secure = body.indexOf('secure_created_staging_child(');
    const hook = body.indexOf('run_staging_created_hook()');

    expect(secure, 'the created staging object must be opened and identity-bound').toBeGreaterThanOrEqual(0);
    expect(hook, 'the owner-local race hook must remain available').toBeGreaterThanOrEqual(0);
    expect(
      hook,
      'the race must run only after staging object identity is retained, so rollback failure can be correlated exactly',
    ).toBeGreaterThan(secure);
  });

  it('has a real-filesystem rollback-failure regression with durable residue evidence', () => {
    expect(
      safetySource,
      'the Rust owner must exercise a real non-empty/otherwise failing rollback, not only successful rollback',
    ).toContain('fn staging_creation_rollback_failure_is_durably_recoverable()');
    expect(
      safetySource,
      'rollback-failure acceptance must prove that the surviving exact staging object is represented by durable recovery evidence',
    ).toContain('staging creation cleanup remains pending');
  });
});
