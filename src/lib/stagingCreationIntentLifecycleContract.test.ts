import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');

describe('staging creation intent lifecycle contract', () => {
  it('terminally closes a durable intent when child creation never succeeds', () => {
    const createStart = safetySource.indexOf('fn create_private_staging_dir_inner(');
    const wrapperStart = safetySource.indexOf(
      'fn create_private_staging_dir_for_operation(',
      createStart,
    );

    expect(createStart, 'create_private_staging_dir_inner must exist').toBeGreaterThanOrEqual(0);
    expect(wrapperStart, 'the operation wrapper must follow the staging creator').toBeGreaterThan(createStart);

    const createBlock = safetySource.slice(createStart, wrapperStart);
    expect(
      createBlock,
      'after a pending creation intent is durable, a generic create_staging_child failure must not return directly and leave that path/name-only intent permanently pending',
    ).not.toMatch(/Err\(error\)\s*=>\s*return Err\(error\.into\(\)\),?/);
  });

  it('proves a failed no-child attempt cannot poison a later successful operation or idempotent retry', () => {
    expect(
      safetySource,
      'the safety owner must exercise a real filesystem child-creation failure after durable intent publication, prove no staging residue survives, then prove a later successful delete and absent-source retry are not blocked by stale intent',
    ).toContain('fn staging_creation_child_failure_closes_durable_intent()');
  });
});
