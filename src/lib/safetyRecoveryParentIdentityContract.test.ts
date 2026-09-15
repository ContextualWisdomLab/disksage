import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');

function sliceBetween(startToken: string, endToken: string): string {
  const start = safetySource.indexOf(startToken);
  const end = safetySource.indexOf(endToken, start);
  expect(start, `${startToken} must exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${endToken} must follow ${startToken}`).toBeGreaterThan(start);
  return safetySource.slice(start, end);
}

describe('staging recovery source-parent identity contract', () => {
  it('persists the staging parent filesystem object in recovery identity', () => {
    const recoveryStruct = sliceBetween(
      'struct StagingCleanupRecovery {',
      'fn staging_cleanup_pending_outcome(',
    );
    const recoveryFactory = sliceBetween(
      'fn staging_cleanup_recovery(',
      'fn cleanup_verified_empty_staging_dir(',
    );

    expect(
      recoveryStruct,
      'legacy receipts must deserialize with an explicit missing parent binding so retry can fail closed instead of dropping the evidence',
    ).toMatch(/source_parent_object_id\s*:\s*Option<String>/);
    expect(
      recoveryStruct,
      'source-parent identity must participate in receipt correlation',
    ).toMatch(/staging_cleanup_recovery_identity[\s\S]*source_parent_object_id/);
    expect(
      recoveryFactory,
      'new receipts must capture the filesystem identity of the actual staging parent',
    ).toMatch(/parent\(\)[\s\S]*filesystem_object_id[\s\S]*source_parent_object_id/);
  });

  it('validates the recorded parent before a missing staging pathname can mean complete', () => {
    const cleanup = sliceBetween(
      'fn cleanup_verified_empty_staging_dir(',
      'fn retry_pending_staging_cleanup(',
    );

    const parentBinding = cleanup.indexOf('source_parent_object_id');
    const parentIdentityRead = cleanup.indexOf('filesystem_object_id', parentBinding);
    const stagingNotFound = cleanup.indexOf('ErrorKind::NotFound');

    expect(parentBinding, 'cleanup must require the recorded source-parent binding').toBeGreaterThanOrEqual(0);
    expect(
      parentIdentityRead,
      'cleanup must read the current parent filesystem identity before inspecting staging absence',
    ).toBeGreaterThan(parentBinding);
    expect(
      stagingNotFound,
      'staging NotFound may be terminal only after exact parent identity validation',
    ).toBeGreaterThan(parentIdentityRead);
    expect(cleanup, 'symlink/reparse parent substitution must remain fail closed').toContain(
      'is_windows_reparse_point',
    );
  });

  it('keeps legacy parent-unbound receipts as explicit fail-closed evidence', () => {
    const retry = sliceBetween(
      'fn retry_pending_staging_cleanup(',
      'fn revalidate_catalog_root_before_staging(',
    );

    expect(
      retry,
      'retry must inspect source-parent binding rather than treating an old receipt as if no recovery evidence existed',
    ).toContain('source_parent_object_id');
    expect(
      retry,
      'an incompatible legacy receipt must produce an explicit protected/recovery error path',
    ).toMatch(/source_parent_object_id[\s\S]{0,1200}SafetyError::(Protected|Trash)/);
  });
});
