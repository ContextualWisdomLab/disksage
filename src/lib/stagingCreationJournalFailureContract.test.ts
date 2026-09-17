import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');

describe('private staging rollback durable-publication contract', () => {
  it('arms crash-durable creation intent before the staging child can exist', () => {
    const createStart = safetySource.indexOf('fn create_private_staging_dir_inner(');
    const wrapperStart = safetySource.indexOf('fn create_private_staging_dir_for_operation(', createStart);

    expect(createStart, 'create_private_staging_dir_inner must exist').toBeGreaterThanOrEqual(0);
    expect(wrapperStart, 'the operation wrapper must follow the staging creator').toBeGreaterThan(createStart);

    const createBlock = safetySource.slice(createStart, wrapperStart);
    const childCreation = createBlock.indexOf('create_staging_child(');
    const durableIntent = createBlock.indexOf('publish_creation_intent(');

    expect(childCreation, 'the staging child creation point must remain explicit').toBeGreaterThanOrEqual(0);
    expect(
      durableIntent,
      'a durable identity-bound creation intent must be published before the child exists; a post-create recovery sidecar is too late',
    ).toBeGreaterThanOrEqual(0);
    expect(
      durableIntent,
      'durable creation intent must precede create_staging_child so every surviving child has pre-existing crash-recovery authority',
    ).toBeLessThan(childCreation);
  });

  it('binds pre-create intent to the reviewed parent, planned child name, target, and catalog authority', () => {
    expect(safetySource).toContain('struct StagingCreationIntent');
    for (const field of [
      'source_parent_object_id',
      'staging_name',
      'target_object_id',
      'catalog_root_object_id',
    ]) {
      expect(
        safetySource,
        `StagingCreationIntent must bind ${field} before child creation`,
      ).toContain(field);
    }
  });

  it('does not return with an untracked residue after every durable recovery publication fails', () => {
    const operationStart = safetySource.indexOf('fn create_private_staging_dir_for_operation(');
    const cleanupStart = safetySource.indexOf('fn cleanup_verified_empty_staging_dir(', operationStart);

    expect(operationStart, 'create_private_staging_dir_for_operation must exist').toBeGreaterThanOrEqual(0);
    expect(cleanupStart, 'cleanup_verified_empty_staging_dir must follow the operation wrapper').toBeGreaterThan(operationStart);

    const operationBlock = safetySource.slice(operationStart, cleanupStart);
    expect(
      operationBlock,
      'after rollback has already failed, exhaustion of later durable publication must not directly return while the exact staging object remains untracked',
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

  it('has a real-filesystem regression for rollback plus exhaustion of every later recovery publication path', () => {
    expect(
      safetySource,
      'the safety owner must force rollback failure plus all later publication failures, then prove the exact staging object is covered by pre-existing durable intent or safely removed through retained authority',
    ).toContain('fn staging_creation_rollback_and_all_recovery_publications_fail_does_not_leave_untracked_residue()');
  });
});
