import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');

describe('private staging rollback journal-failure contract', () => {
  it('does not return with an untracked residue when rollback failed and journal publication also fails', () => {
    const operationStart = safetySource.indexOf('fn create_private_staging_dir_for_operation(');
    const cleanupStart = safetySource.indexOf('fn cleanup_verified_empty_staging_dir(', operationStart);

    expect(operationStart, 'create_private_staging_dir_for_operation must exist').toBeGreaterThanOrEqual(0);
    expect(cleanupStart, 'cleanup_verified_empty_staging_dir must follow the operation wrapper').toBeGreaterThan(operationStart);

    const operationBlock = safetySource.slice(operationStart, cleanupStart);
    expect(
      operationBlock,
      'after rollback has already failed, journal publication failure must not directly return while the exact staging residue remains untracked',
    ).not.toMatch(
      /Err\(journal_error\)\s*=>\s*Err\(SafetyError::Trash\(format!\(\s*"staging creation cleanup remains pending; durable recovery evidence publication failed:/s,
    );
  });

  it('has a real-filesystem regression for simultaneous rollback and durable-publication failure', () => {
    expect(
      safetySource,
      'the safety owner must force both rollback failure and journal publication failure against a real staging directory',
    ).toContain('fn staging_creation_rollback_and_journal_failure_does_not_leave_untracked_residue()');
  });
});
