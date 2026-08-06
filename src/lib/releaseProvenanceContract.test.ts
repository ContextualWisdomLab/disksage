import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/**
 * Read one UTF-8 repository file from the source-controlled project root.
 *
 * Resolving from this module keeps the governance contract deterministic when
 * Vitest runs from an IDE, a parent workspace, or an isolated CI directory.
 */
function readRepositoryFile(relativePath: string): string {
  return readFileSync(resolve(repositoryRoot, relativePath), 'utf8');
}

/**
 * Return one top-level GitHub Actions job block without parsing untrusted YAML.
 *
 * Release governance tests need only stable job boundaries. Restricting the
 * scanner to two-space-indented job keys keeps the assertion dependency-free
 * and makes an absent or duplicate contract fail loudly.
 */
function extractWorkflowJob(workflow: string, jobName: string): string {
  const marker = `\n  ${jobName}:\n`;
  const start = workflow.indexOf(marker);
  if (start < 0) {
    throw new Error(`Missing workflow job: ${jobName}`);
  }

  const contentStart = start + marker.length;
  const remaining = workflow.slice(contentStart);
  const nextJobOffset = remaining.search(/\n  [a-zA-Z0-9_-]+:\n/);
  return nextJobOffset < 0 ? remaining : remaining.slice(0, nextJobOffset);
}

describe('release artifact provenance contract', () => {
  it('makes exact release provenance a tag-only gate before publication', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    const buildJob = extractWorkflowJob(workflow, 'build');
    const attestJob = extractWorkflowJob(workflow, 'attest-release');
    const publishJob = extractWorkflowJob(workflow, 'publish-release');

    expect(buildJob).toContain('name: Upload release artifact set');
    expect(buildJob).toContain('name: release-disksage-${{ matrix.os }}');
    expect(buildJob).not.toContain('softprops/action-gh-release');

    expect(attestJob).toContain("if: startsWith(github.ref, 'refs/tags/')");
    expect(attestJob).toContain('needs: build');
    expect(attestJob).toContain('contents: read');
    expect(attestJob).toContain('id-token: write');
    expect(attestJob).toContain('attestations: write');
    expect(attestJob).toContain(
      'actions/download-artifact@37930b1c2abaa49bbe596cd826c3c89aef350131',
    );
    expect(attestJob).toContain('pattern: release-disksage-*');
    expect(attestJob).toContain(
      'actions/attest@59d89421af93a897026c735860bf21b6eb4f7b26',
    );
    expect(attestJob).toContain('subject-path: release-artifacts/**/*');
    expect(attestJob).toContain('name: Verify release artifact checksums');
    expect(attestJob).toContain(
      "require_exactly_one_path '*/bundle/nsis/*.exe' 'Windows NSIS bundle'",
    );
    expect(attestJob).not.toContain("require_exactly_one '*.exe'");
    expect(attestJob.indexOf('name: Verify release artifact checksums')).toBeLessThan(
      attestJob.indexOf('actions/attest@59d89421af93a897026c735860bf21b6eb4f7b26'),
    );

    expect(publishJob).toContain("if: startsWith(github.ref, 'refs/tags/')");
    expect(publishJob).toContain('needs: attest-release');
    expect(publishJob).toContain('contents: write');
    expect(publishJob).toContain(
      'actions/download-artifact@37930b1c2abaa49bbe596cd826c3c89aef350131',
    );
    expect(publishJob).toContain('pattern: release-disksage-*');
    expect(publishJob).toContain(
      'softprops/action-gh-release@3d0d9888cb7fd7b750713d6e236d1fcb99157228',
    );
  });

  it('documents buyer-verifiable provenance and authoritative standards', () => {
    const doctoring = readRepositoryFile(
      'docs/doctoring/release-artifact-provenance.md',
    );
    const changelog = readRepositoryFile('CHANGELOG.md');

    expect(doctoring).toContain('# Release artifact provenance');
    expect(doctoring).toContain(
      'gh attestation verify PATH/TO/ARTIFACT -R ContextualWisdomLab/disksage',
    );
    expect(doctoring).toContain('SLSA Provenance v1');
    expect(doctoring).toContain('in-toto Statement v1');
    expect(doctoring).toContain('APA 7th references');
    expect(doctoring).toContain('59d89421af93a897026c735860bf21b6eb4f7b26');
    expect(doctoring).toContain('37930b1c2abaa49bbe596cd826c3c89aef350131');
    expect(changelog).toContain('buyer-verifiable release artifact provenance');
  });
});
