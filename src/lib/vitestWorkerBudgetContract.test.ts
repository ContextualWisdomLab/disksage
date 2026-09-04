import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/** Read the Vitest configuration with stable line endings for worker-budget assertions. */
function readVitestConfig(): string {
  return readFileSync(resolve(repositoryRoot, 'vitest.config.ts'), 'utf8').replace(/\r\n?/g, '\n');
}

describe('Vitest hosted-runner worker budget', () => {
  it('bounds CI worker concurrency without disabling file isolation or test files', () => {
    const config = readVitestConfig();

    expect(config).toContain('maxWorkers: process.env.CI ? 2 : undefined');
    expect(config).not.toContain('fileParallelism: false');
    expect(config).not.toContain('pool: "threads"');
    expect(config).not.toContain("pool: 'threads'");
  });
});
