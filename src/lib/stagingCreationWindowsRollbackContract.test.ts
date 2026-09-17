import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');

describe('Windows staging rollback exact-object contract', () => {
  it('does not authorize rollback deletion from the staging pathname alone', () => {
    const guardStart = safetySource.indexOf('struct StagingCreationGuard');
    const errorStart = safetySource.indexOf(
      '#[derive(Debug)]\nenum StagingCreationError',
      guardStart,
    );

    expect(guardStart, 'StagingCreationGuard must exist').toBeGreaterThanOrEqual(0);
    expect(errorStart, 'StagingCreationError must follow the guard').toBeGreaterThan(guardStart);

    const guardBlock = safetySource.slice(guardStart, errorStart);
    expect(
      guardBlock,
      'Windows/non-Unix rollback must not remove a possibly substituted staging pathname without exact-object authority',
    ).not.toMatch(
      /#\[cfg\(not\(unix\)\)\][\s\S]{0,500}std::fs::remove_dir\(&self\.path\)/,
    );
  });

  it('proves a Windows staging-path replacement cannot be deleted by rollback', () => {
    expect(
      safetySource,
      'the safety owner must exercise a real Windows filesystem substitution after staging identity capture and prove rollback never deletes the replacement object',
    ).toContain('fn staging_creation_windows_substitution_does_not_delete_replacement()');
  });
});
