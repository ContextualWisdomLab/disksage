import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');

describe('post-mutation recovery durability contract', () => {
  it('durably synchronizes every accepted journal append before returning success', () => {
    const start = safetySource.indexOf('pub fn journal_append(');
    const end = safetySource.indexOf('pub fn journal_recent(', start);

    expect(start, 'journal_append must exist').toBeGreaterThanOrEqual(0);
    expect(end, 'journal_recent must follow journal_append').toBeGreaterThan(start);

    const body = safetySource.slice(start, end);
    const write = body.indexOf('.write_all(');
    const fileSync = body.indexOf('.sync_all()', write);

    expect(write).toBeGreaterThanOrEqual(0);
    expect(fileSync, 'journal bytes must reach stable storage before append succeeds').toBeGreaterThan(
      write,
    );
  });

  it('does not replay an older cleanup-pending receipt after a newer terminal receipt', () => {
    const start = safetySource.indexOf('fn retry_pending_staging_cleanup(');
    const end = safetySource.indexOf('fn revalidate_catalog_root_before_staging(', start);

    expect(start, 'retry_pending_staging_cleanup must exist').toBeGreaterThanOrEqual(0);
    expect(end, 'revalidation helper must follow recovery retry').toBeGreaterThan(start);

    const body = safetySource.slice(start, end);
    expect(body).not.toContain('.find_map(|entry| parse_staging_cleanup_pending(&entry.outcome))');
    expect(body).toMatch(/outcome[\s\S]{0,240}"ok"/);
  });
});
