import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/** Read the release workflow with stable line endings for retry-identity assertions. */
function readReleaseWorkflow(): string {
  return readFileSync(resolve(repositoryRoot, '.github/workflows/release.yml'), 'utf8').replace(/\r\n?/g, '\n');
}

/** Return one top-level job block from the release workflow. */
function job(workflow: string, name: string, nextName: string): string {
  const start = workflow.indexOf(`  ${name}:`);
  const end = workflow.indexOf(`  ${nextName}:`, start);
  expect(start).toBeGreaterThanOrEqual(0);
  expect(end).toBeGreaterThan(start);
  return workflow.slice(start, end);
}

describe('release artifact retry identity', () => {
  it('keeps build artifacts stable across failed-job rerun attempts', () => {
    const workflow = readReleaseWorkflow();
    const build = job(workflow, 'build', 'download-artifact-pr-compat');

    expect(build).toContain('name: release-disksage-${{ matrix.os }}-${{ github.run_id }}');
    expect(build).toContain('overwrite: true');
    expect(build).not.toContain('name: release-disksage-${{ matrix.os }}-${{ github.run_attempt }}');
  });

  it('binds every build-artifact consumer to the stable workflow-run identity', () => {
    const workflow = readReleaseWorkflow();
    const prCompat = job(workflow, 'download-artifact-pr-compat', 'attest-release');
    const attest = job(workflow, 'attest-release', 'publish-release');
    const publish = job(workflow, 'publish-release', 'gpu-build');

    for (const consumer of [prCompat, attest, publish]) {
      expect(consumer).toContain('release-disksage-*-${{ github.run_id }}');
      expect(consumer).not.toContain('release-disksage-*-${{ github.run_attempt }}');
    }
    expect(prCompat).toContain('verify-release-artifacts.sh release-artifacts "${{ github.run_id }}"');
    expect(attest).toContain('verify-release-artifacts.sh release-artifacts "${{ github.run_id }}"');
  });

  it('keeps the attested SBOM address stable and overwrite-safe across reruns', () => {
    const workflow = readReleaseWorkflow();
    const attest = job(workflow, 'attest-release', 'publish-release');
    const publish = job(workflow, 'publish-release', 'gpu-build');

    expect(attest).toContain('name: release-sbom-${{ github.run_id }}');
    expect(attest).toContain('overwrite: true');
    expect(publish).toContain('name: release-sbom-${{ github.run_id }}');
    expect(workflow).not.toContain('release-sbom-${{ github.run_attempt }}');
  });
});
