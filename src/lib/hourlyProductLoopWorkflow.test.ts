import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const workflow = readFileSync(
  new URL('../../.github/workflows/hourly-product-loop.yml', import.meta.url),
  'utf8',
);

describe('hourly product loop workflow authority', () => {
  it('keeps the repository-local entry point manual-only', () => {
    expect(workflow).toContain('workflow_dispatch:');
    expect(workflow).not.toMatch(/^\s*schedule:\s*$/mu);
  });

  it('does not carry repository write permissions into the product caller', () => {
    expect(workflow).toContain('contents: read');
    expect(workflow).toContain('id-token: write');
    expect(workflow).not.toContain('contents: write');
    expect(workflow).not.toContain('pull-requests: write');
  });

  it('delegates review and repair authority to the exact central owner revision', () => {
    expect(workflow).toContain(
      'ContextualWisdomLab/.github/.github/workflows/pr-review-fix-scheduler.yml@e6334e229581a918e2f22de18733b76fa65d7e71',
    );
    expect(workflow).not.toContain('/v1/models');
    expect(workflow).not.toContain('/v1/chat/completions');
  });
});
