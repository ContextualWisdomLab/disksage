import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/** Read one UTF-8 repository file from the source-controlled project root. */
function readRepositoryFile(relativePath: string): string {
  return readFileSync(resolve(repositoryRoot, relativePath), 'utf8').replace(/\r\n?/g, '\n');
}

describe('release pull-request lifecycle contract', () => {
  it('restarts verification when a draft becomes reviewable and cleans up inactive PR runs', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    expect(workflow).toContain(
      'types: [opened, synchronize, reopened, ready_for_review, converted_to_draft, closed]',
    );
    expect(workflow).toContain(
      "if: ${{ github.event_name != 'pull_request' || (!github.event.pull_request.draft && github.event.action != 'closed') }}",
    );
  });

  it('cancels only stale first-attempt PR runs while preserving explicit reruns', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    expect(workflow).toContain(
      'group: ${{ github.workflow }}-${{ github.repository }}-${{ github.event.pull_request.number || github.ref }}',
    );
    expect(workflow).toContain(
      "cancel-in-progress: ${{ github.event_name == 'pull_request' && github.run_attempt == 1 }}",
    );
  });
});
