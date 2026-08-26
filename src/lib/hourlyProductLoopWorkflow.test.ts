import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const workflow = readFileSync(
  new URL('../../.github/workflows/hourly-product-loop.yml', import.meta.url),
  'utf8',
);

describe('hourly product loop workflow authority', () => {
  it('does not autonomously schedule a model-only reviewer that is not pinned OpenCode', () => {
    expect(workflow).toContain('workflow_dispatch:');
    expect(workflow).not.toMatch(/^\s*schedule:\s*$/mu);
  });

  it('remains read-only when manually dispatched', () => {
    expect(workflow).toContain('contents: read');
    expect(workflow).toContain('pull-requests: read');
    expect(workflow).not.toContain('contents: write');
    expect(workflow).not.toContain('pull-requests: write');
  });
});
