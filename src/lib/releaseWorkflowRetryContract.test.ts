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
  it('cancels superseded PR builds while preserving tag and manual releases', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    expect(workflow).toContain(
      "cancel-in-progress: ${{ github.event_name == 'pull_request' && github.run_attempt == 1 }}",
    );
  });

  it('binds upload and download artifact names to the current rerun attempt', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    const verifier = readRepositoryFile('.github/scripts/verify-release-artifacts.sh');
    expect(workflow).toContain(
      'name: release-disksage-${{ matrix.os }}-${{ github.run_attempt }}',
    );
    expect(
      workflow.split('pattern: release-disksage-*-${{ github.run_attempt }}').length - 1,
    ).toBe(3);
    expect(verifier).toContain('release-disksage-windows-2022-${run_attempt}');
    expect(verifier).not.toContain('release-disksage-windows-latest-${run_attempt}');
  });

  it('documents trigger-aware release concurrency in authoritative evidence', () => {
    const doctoring = readRepositoryFile('docs/doctoring/release-artifact-provenance.md');
    const changelog = readRepositoryFile('CHANGELOG.md');
    expect(doctoring).toContain('superseded pull-request build');
    expect(doctoring).toContain("github.event_name == 'pull_request' && github.run_attempt == 1");
    expect(doctoring).toContain('Re-run all jobs');
    expect(changelog).toContain('retry-safe release concurrency');
  });
});
