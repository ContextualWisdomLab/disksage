import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/** Reads the source-controlled release workflow with normalized line endings for stable assertions. */
function readReleaseWorkflow(): string {
  return readFileSync(resolve(repositoryRoot, '.github/workflows/release.yml'), 'utf8').replace(/\r\n?/g, '\n');
}

describe('tag release artifact verifier contract', () => {
  it('runs the shared exact build-artifact verifier before generating the SBOM', () => {
    const workflow = readReleaseWorkflow();
    const attestStart = workflow.indexOf('\n  attest-release:\n');
    const publishStart = workflow.indexOf('\n  publish-release:\n', attestStart);
    expect(attestStart).toBeGreaterThanOrEqual(0);
    expect(publishStart).toBeGreaterThan(attestStart);

    const attestJob = workflow.slice(attestStart, publishStart);
    const sharedVerifier = 'bash .github/scripts/verify-release-artifacts.sh release-artifacts "${{ github.run_attempt }}"';
    const verifierOffset = attestJob.indexOf(sharedVerifier);
    const sbomOffset = attestJob.indexOf('- name: Generate and validate source-bound SBOM');

    expect(verifierOffset).toBeGreaterThanOrEqual(0);
    expect(sbomOffset).toBeGreaterThan(verifierOffset);
  });
});