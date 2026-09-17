import { createHash } from 'node:crypto';
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const operationalAssetNames = [
  'disksage-cloud-plan-linux-x86_64',
  'disksage-duplicate-audit-linux-x86_64',
  'disksage-cloud-plan-windows-x86_64.exe',
  'disksage-duplicate-audit-windows-x86_64.exe',
  'disksage-cloud-plan-macos-arm64',
  'disksage-duplicate-audit-macos-arm64',
  'disksage-parallels-disk-reclaim-macos-arm64',
] as const;

/** Read one UTF-8 repository file from the source-controlled project root. */
function readRepositoryFile(relativePath: string): string {
  return readFileSync(resolve(repositoryRoot, relativePath), 'utf8');
}

/** Return one top-level GitHub Actions job block after normalizing line endings. */
function extractWorkflowJob(workflow: string, jobName: string): string {
  const normalizedWorkflow = workflow.replace(/\r\n?/g, '\n');
  const marker = `\n  ${jobName}:\n`;
  const start = normalizedWorkflow.indexOf(marker);
  if (start < 0) throw new Error(`Missing workflow job: ${jobName}`);
  const remaining = normalizedWorkflow.slice(start + marker.length);
  const nextJobOffset = remaining.search(/\n  [a-zA-Z0-9_-]+:\n/);
  return nextJobOffset < 0 ? remaining : remaining.slice(0, nextJobOffset);
}

/** Extract one literal Bash run block from a named workflow step. */
function extractWorkflowRunScript(job: string, stepName: string): string {
  const normalizedJob = job.replace(/\r\n?/g, '\n');
  const stepMarker = `      - name: ${stepName}\n`;
  const stepStart = normalizedJob.indexOf(stepMarker);
  if (stepStart < 0) throw new Error(`Missing workflow step: ${stepName}`);
  const runMarker = '        run: |\n';
  const runStart = normalizedJob.indexOf(runMarker, stepStart);
  if (runStart < 0) throw new Error(`Missing literal run block: ${stepName}`);
  const remaining = normalizedJob.slice(runStart + runMarker.length);
  const nextStepOffset = remaining.search(/\n      - (?:name:|uses:)/);
  const script = nextStepOffset < 0 ? remaining : remaining.slice(0, nextStepOffset);
  return script
    .split('\n')
    .map((line) => (line.startsWith('          ') ? line.slice(10) : line))
    .join('\n');
}

/** Create one complete release-artifact tree for admission-boundary tests. */
function createReleaseArtifactFixture(): string {
  const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-provenance-'));
  const artifactRoot = join(fixtureRoot, 'release-artifacts');
  mkdirSync(join(artifactRoot, 'sbom'), { recursive: true });
  writeFileSync(
    join(artifactRoot, 'sbom', 'disksage.spdx.json'),
    JSON.stringify({
      spdxVersion: 'SPDX-2.3',
      dataLicense: 'CC0-1.0',
      SPDXID: 'SPDXRef-DOCUMENT',
      name: 'disksage-deadbeef',
      documentNamespace: 'https://github.com/ContextualWisdomLab/disksage/sbom/deadbeef',
      creationInfo: { created: '2000-01-01T00:00:00.000Z', creators: ['Tool: disksage-release-sbom'] },
      documentDescribes: ['SPDXRef-Cargo-root'],
      packages: [{
        SPDXID: 'SPDXRef-Cargo-root',
        name: 'cargo:disksage',
        versionInfo: '0.1.0',
        downloadLocation: 'NOASSERTION',
        filesAnalyzed: false,
        licenseConcluded: 'NOASSERTION',
        licenseDeclared: 'NOASSERTION',
        supplier: 'NOASSERTION',
      }],
      relationships: [],
      documentComment: 'Dependency inventory bound to source revision deadbeef.',
    }),
  );
  for (const bundlePath of [
    'ubuntu/bundle/deb/disksage.deb',
    'ubuntu/bundle/appimage/disksage.AppImage',
    'windows/bundle/msi/disksage.msi',
    'windows/bundle/nsis/disksage-setup.exe',
    'macos/bundle/dmg/disksage.dmg',
  ]) {
    const absolutePath = join(artifactRoot, bundlePath);
    mkdirSync(dirname(absolutePath), { recursive: true });
    writeFileSync(absolutePath, `bundle:${bundlePath}`);
  }
  for (const assetName of operationalAssetNames) {
    const platformDirectory = assetName.includes('windows')
      ? 'windows'
      : assetName.includes('macos')
        ? 'macos'
        : 'ubuntu';
    const assetPath = join(artifactRoot, platformDirectory, assetName);
    const bytes = Buffer.from(`operational-cli:${assetName}`);
    mkdirSync(dirname(assetPath), { recursive: true });
    writeFileSync(assetPath, bytes);
    writeFileSync(
      `${assetPath}.sha256`,
      `${createHash('sha256').update(bytes).digest('hex')}  ${assetName}\n`,
    );
  }
  return fixtureRoot;
}

/** Execute the exact source-controlled release admission script in one fixture. */
function runReleaseArtifactVerifier(fixtureRoot: string) {
  const workflow = readRepositoryFile('.github/workflows/release.yml');
  const attestJob = extractWorkflowJob(workflow, 'attest-release');
  const verifier = extractWorkflowRunScript(attestJob, 'Verify release artifact checksums');
  return spawnSync('bash', ['-c', verifier], {
    cwd: fixtureRoot,
    encoding: 'utf8',
    env: { ...process.env, GITHUB_WORKSPACE: repositoryRoot },
  });
}

describe('release artifact provenance contract', () => {
  it('separates build, attestation, and publication authority on exact artifacts', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    const buildJob = extractWorkflowJob(workflow, 'build');
    const attestJob = extractWorkflowJob(workflow, 'attest-release');
    const publishJob = extractWorkflowJob(workflow, 'publish-release');

    expect(buildJob).toContain('contents: read');
    expect(buildJob).toContain('name: Upload release artifact set');
    expect(buildJob).toContain('name: release-disksage-${{ matrix.os }}');
    expect(buildJob).not.toContain('softprops/action-gh-release');
    expect(buildJob).toContain('if ! "$asset_path" --help');
    expect(buildJob).not.toContain('--help 2>&1 || true');

    expect(attestJob).toContain("if: startsWith(github.ref, 'refs/tags/')");
    expect(attestJob).toContain('needs: build');
    expect(attestJob).toContain('contents: read');
    expect(attestJob).toContain('id-token: write');
    expect(attestJob).toContain('attestations: write');
    expect(attestJob).not.toContain('artifact-metadata: write');
    expect(attestJob).toContain('pattern: release-disksage-*');
    expect(attestJob).toContain('merge-multiple: false');
    expect(attestJob).toContain(
      'actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6',
    );
    expect(attestJob).toContain('subject-path: release-artifacts/**/*');
    expect(attestJob).toContain('Generate and validate source-bound SBOM');
    expect(attestJob).toContain('disksage.spdx.json');
    expect(attestJob).toContain('expected exactly 20 regular files');
    expect(attestJob).toContain('require_exactly_one_file "$required_name"');
    expect(attestJob).toContain('require_exactly_one_file "$required_name.sha256"');

    expect(publishJob).toContain("if: startsWith(github.ref, 'refs/tags/')");
    expect(publishJob).toContain('needs: attest-release');
    expect(publishJob).toContain('contents: write');
    expect(publishJob).toContain('pattern: release-disksage-*');
    expect(publishJob).toContain('merge-multiple: false');
    expect(publishJob).toContain(
      'softprops/action-gh-release@3d0d9888cb7fd7b750713d6e236d1fcb99157228',
    );
  });

  it.runIf(process.platform !== 'win32')(
    'rejects a checksum record authenticating a decoy rather than its adjacent CLI',
    () => {
      const fixtureRoot = createReleaseArtifactFixture();
      try {
        const linuxDirectory = join(fixtureRoot, 'release-artifacts', 'ubuntu');
        const checksumPath = join(
          linuxDirectory,
          'disksage-cloud-plan-linux-x86_64.sha256',
        );
        const decoyName = 'unpublished-decoy';
        const decoyBytes = Buffer.from('not-the-published-cli');
        writeFileSync(join(linuxDirectory, decoyName), decoyBytes);
        writeFileSync(
          checksumPath,
          `${createHash('sha256').update(decoyBytes).digest('hex')}  ${decoyName}\n`,
        );

        const result = runReleaseArtifactVerifier(fixtureRoot);
        expect(result.status).not.toBe(0);
        expect(result.stderr).toContain('must reference its adjacent operational CLI');
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );

  it.runIf(process.platform !== 'win32')(
    'rejects one required CLI duplicated across preserved artifact namespaces',
    () => {
      const fixtureRoot = createReleaseArtifactFixture();
      try {
        const duplicatedName = 'disksage-cloud-plan-linux-x86_64';
        const sourcePath = join(fixtureRoot, 'release-artifacts', 'ubuntu', duplicatedName);
        const duplicatePath = join(fixtureRoot, 'release-artifacts', 'windows', duplicatedName);
        mkdirSync(dirname(duplicatePath), { recursive: true });
        writeFileSync(duplicatePath, readFileSync(sourcePath));

        const result = runReleaseArtifactVerifier(fixtureRoot);
        expect(result.status).not.toBe(0);
        expect(result.stderr).toContain(
          `Expected exactly one release artifact named ${duplicatedName}, found 2.`,
        );
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );

  it('keeps buyer verification and authoritative provenance references discoverable', () => {
    const doctoring = readRepositoryFile('docs/doctoring/release-artifact-provenance.md');
    const changelog = readRepositoryFile('CHANGELOG.md');

    expect(doctoring).toContain(
      'gh attestation verify PATH/TO/ARTIFACT -R ContextualWisdomLab/disksage',
    );
    expect(doctoring).toContain('SLSA Provenance v1');
    expect(doctoring).toContain('in-toto Statement v1');
    expect(doctoring).toContain('APA 7th references');
    expect(changelog).toContain('buyer-verifiable release artifact provenance');
  });
});
