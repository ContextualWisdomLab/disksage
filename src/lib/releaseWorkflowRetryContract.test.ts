import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/** Read one UTF-8 repository file from the source-controlled project root. */
function readRepositoryFile(relativePath: string): string {
  return readFileSync(resolve(repositoryRoot, relativePath), 'utf8').replace(/\r\n?/g, '\n');
}

describe('release workflow retry contract', () => {
  it('preserves in-flight release builds across newer heads and explicit reruns', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    expect(workflow).toContain('cancel-in-progress: false');
    expect(workflow).not.toContain('cancel-in-progress: true');
  });

  it('binds release artifact names to the stable workflow run identity', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    expect(workflow).toContain(
      'name: release-disksage-${{ matrix.os }}-${{ github.run_id }}',
    );
    expect(workflow.split('pattern: release-disksage-*-${{ github.run_id }}').length - 1).toBe(3);
    expect(workflow).not.toContain('release-disksage-${{ matrix.os }}-${{ github.run_attempt }}');
  });

  it('documents non-cancelling release concurrency in authoritative evidence', () => {
    const doctoring = readRepositoryFile('docs/doctoring/release-artifact-provenance.md');
    const changelog = readRepositoryFile('CHANGELOG.md');
    expect(doctoring).toContain('release concurrency never cancels an in-flight build');
    expect(doctoring).toContain('cancel-in-progress: false');
    expect(changelog).toContain('retry-safe release concurrency');
  });
});
