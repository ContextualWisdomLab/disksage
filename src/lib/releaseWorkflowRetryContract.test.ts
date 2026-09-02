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
  it('cancels stale first attempts without self-cancelling explicit reruns', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    expect(workflow).toContain("cancel-in-progress: ${{ github.run_attempt == 1 }}");
    expect(workflow).not.toContain('cancel-in-progress: true');
  });

  it('binds upload and download artifact names to the current rerun attempt', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    expect(workflow).toContain(
      'name: release-disksage-${{ matrix.os }}-${{ github.run_attempt }}',
    );
    expect(
      workflow.split('pattern: release-disksage-*-${{ github.run_attempt }}').length - 1,
    ).toBe(3);
  });

  it('keeps verifier artifact directories aligned with the pinned build matrix', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    const verifier = readRepositoryFile('.github/scripts/verify-release-artifacts.sh');

    for (const runner of ['ubuntu-22.04', 'windows-2022', 'macos-latest']) {
      expect(workflow).toContain(`- os: ${runner}`);
      expect(verifier).toContain(`release-disksage-${runner}-\${run_attempt}`);
    }
    expect(verifier).not.toContain('release-disksage-windows-latest-${run_attempt}');
  });

  it('documents retry-safe concurrency in authoritative evidence', () => {
    const doctoring = readRepositoryFile('docs/doctoring/release-artifact-provenance.md');
    const changelog = readRepositoryFile('CHANGELOG.md');
    expect(doctoring).toContain('explicit rerun attempts do not cancel themselves');
    expect(doctoring).toContain('github.run_attempt == 1');
    expect(changelog).toContain('retry-safe release concurrency');
  });
});
