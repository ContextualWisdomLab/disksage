import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');

describe('private staging rollback durable-publication contract', () => {
  it('does not return with an untracked residue after every durable recovery publication fails', () => {
    const operationStart = safetySource.indexOf('fn create_private_staging_dir_for_operation(');
    const cleanupStart = safetySource.indexOf('fn cleanup_verified_empty_staging_dir(', operationStart);

    expect(operationStart, 'create_private_staging_dir_for_operation must exist').toBeGreaterThanOrEqual(0);
    expect(cleanupStart, 'cleanup_verified_empty_staging_dir must follow the operation wrapper').toBeGreaterThan(operationStart);

    const operationBlock = safetySource.slice(operationStart, cleanupStart);
    expect(
      operationBlock,
      'after rollback has already failed, exhaustion of both primary and fallback durable publication must not directly return while the exact staging object remains untracked',
    ).not.toMatch(
      /Err\(journal_error\)\s*=>\s*Err\(SafetyError::Trash\(format!\(\s*"staging creation cleanup remains pending; durable recovery evidence publication failed in both journals:/s,
    );
  });

  it('keeps the primary-journal failure recovery regression', () => {
    expect(
      safetySource,
      'the safety owner must retain the real-filesystem regression proving a primary journal failure can fall back to durable object-bound recovery evidence',
    ).toContain('fn staging_creation_rollback_and_journal_failure_does_not_leave_untracked_residue()');
  });

  it('has a real-filesystem regression for rollback plus exhaustion of every durable recovery publication path', () => {
    expect(
      safetySource,
      'the safety owner must force rollback failure plus both primary and fallback journal publication failures, then prove the exact staging object is either durably recoverable or safely removed through retained authority',
    ).toContain('fn staging_creation_rollback_and_all_recovery_publications_fail_does_not_leave_untracked_residue()');
  });
});
