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
    ).toBe(2);
  });

  it('pins the Windows installer build to the stable VS2022 runner', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    expect(workflow).toContain('- os: windows-2022');
    expect(workflow).not.toContain('- os: windows-latest');
  });

  it('does not spend native packaging runners on test-only source changes', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    const rustSource = workflow.indexOf('      - "src-tauri/**"');
    const rustTests = workflow.indexOf('      - "!src-tauri/tests/**"');
    const frontendSource = workflow.indexOf('      - "src/**"');
    const frontendTests = workflow.indexOf('      - "!src/**/*.test.ts"');

    expect(rustSource).toBeGreaterThanOrEqual(0);
    expect(rustTests).toBeGreaterThan(rustSource);
    expect(frontendSource).toBeGreaterThanOrEqual(0);
    expect(frontendTests).toBeGreaterThan(frontendSource);
    expect(workflow).toContain('      - ".github/workflows/release.yml"');
    expect(workflow).toContain('      - "package.json"');
    expect(workflow).toContain('      - "package-lock.json"');
  });

  it('fails closed when an operational CLI help smoke exits nonzero or writes stderr', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    expect(workflow).not.toContain('2>&1 || true');
    expect(workflow).toContain('help_stderr="$(mktemp)"');
    expect(workflow).toContain('if ! help_stdout="$("$asset_path" --help 2>"$help_stderr")"; then');
    expect(workflow).toContain('if [[ -s "$help_stderr" ]]; then');
    expect(workflow).toContain('rm -f "$help_stderr"');
  });

  it('documents retry-safe concurrency in authoritative evidence', () => {
    const doctoring = readRepositoryFile('docs/doctoring/release-artifact-provenance.md');
    const changelog = readRepositoryFile('CHANGELOG.md');
    expect(doctoring).toContain('explicit rerun attempts do not cancel themselves');
    expect(doctoring).toContain('github.run_attempt == 1');
    expect(changelog).toContain('retry-safe release concurrency');
  });
});
