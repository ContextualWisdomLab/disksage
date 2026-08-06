import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/** Read the release workflow from the source-controlled repository root. */
function readReleaseWorkflow(): string {
  return readFileSync(
    resolve(repositoryRoot, '.github/workflows/release.yml'),
    'utf8',
  ).replace(/\r\n?/g, '\n');
}

describe('release workflow retry contract', () => {
  it('cancels stale first attempts without self-cancelling explicit reruns', () => {
    const workflow = readReleaseWorkflow();

    expect(workflow).toContain(
      "cancel-in-progress: ${{ github.run_attempt == 1 }}",
    );
    expect(workflow).not.toContain('cancel-in-progress: true');
  });
});
