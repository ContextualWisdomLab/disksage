import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const workflow = readFileSync(
  new URL('../../.github/workflows/test.yml', import.meta.url),
  'utf8',
);

describe('coverage evidence documentation path contract', () => {
  it('runs Test when the executable coverage contract documentation changes', () => {
    const contractPath = 'docs/development/coverage-evidence.md';
    expect(workflow.split(contractPath).length - 1).toBe(2);
  });
});
