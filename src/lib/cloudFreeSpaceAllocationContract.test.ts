import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const source = readFileSync('src-tauri/src/cloud_app_managed.rs', 'utf8');

describe('cloud free-space accounting contract', () => {
  it('requires observed allocation evidence for source-eviction credit', () => {
    expect(
      source,
      'logical file length is not evidence of bytes actually reclaimed from the local filesystem',
    ).toContain('source_allocated_bytes_before');
    expect(source).toContain('observed_allocation_reduction_bytes');
    expect(source).not.toMatch(/SourceEvicted\s*=>\s*source_logical_bytes/);
  });

  it('bounds credited bytes by the source allocation that existed before eviction', () => {
    expect(source).toMatch(
      /SourceEvicted\s*=>\s*observed_allocation_reduction_bytes\.min\(source_allocated_bytes_before\)/,
    );
  });
});
