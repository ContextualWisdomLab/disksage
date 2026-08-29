import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

function extractWorkflowJob(workflow: string, jobName: string): string {
  const normalized = workflow.replace(/\r\n?/g, '\n');
  const marker = `\n  ${jobName}:\n`;
  const start = normalized.indexOf(marker);
  if (start < 0) throw new Error(`Missing workflow job: ${jobName}`);
  const remaining = normalized.slice(start + marker.length);
  const nextJobOffset = remaining.search(/\n  [a-zA-Z0-9_-]+:\n/);
  return nextJobOffset < 0 ? remaining : remaining.slice(0, nextJobOffset);
}

describe('release SBOM publication contract', () => {
  it('publishes the exact source-bound SBOM only after provenance succeeds', () => {
    const workflow = readFileSync(resolve(repositoryRoot, '.github/workflows/release.yml'), 'utf8');
    const attestJob = extractWorkflowJob(workflow, 'attest-release');
    const publishJob = extractWorkflowJob(workflow, 'publish-release');

    const provenanceOffset = attestJob.indexOf('name: Generate GitHub build provenance');
    const sbomUploadOffset = attestJob.indexOf('name: Upload attested release SBOM');
    expect(provenanceOffset).toBeGreaterThanOrEqual(0);
    expect(sbomUploadOffset).toBeGreaterThan(provenanceOffset);
    expect(attestJob).toContain('name: release-sbom-${{ github.run_attempt }}');
    expect(attestJob).toContain('path: release-artifacts/sbom/disksage.spdx.json');
    expect(attestJob).toContain('if-no-files-found: error');

    expect(publishJob).toContain('name: Download attested release SBOM');
    expect(publishJob).toContain('name: release-sbom-${{ github.run_attempt }}');
    expect(publishJob).toContain('path: release-artifacts/sbom');
    expect(publishJob).toContain('files: release-artifacts/**/*');
  });
});
