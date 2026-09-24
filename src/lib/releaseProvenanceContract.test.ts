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
const runAttempt = '1';
const platformDirectories = {
  linux: `release-disksage-ubuntu-22.04-${runAttempt}`,
  windows: `release-disksage-windows-2022-${runAttempt}`,
  macos: `release-disksage-macos-latest-${runAttempt}`,
} as const;
const operationalAssetNames = [
  'disksage-cloud-plan-linux-x86_64',
  'disksage-duplicate-audit-linux-x86_64',
  'disksage-cloud-plan-windows-x86_64.exe',
  'disksage-duplicate-audit-windows-x86_64.exe',
  'disksage-cloud-plan-macos-arm64',
  'disksage-duplicate-audit-macos-arm64',
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

/** Create one complete 17-file release-artifact tree for admission-boundary tests. */
function createReleaseArtifactFixture(): string {
  const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-provenance-'));
  const artifactRoot = join(fixtureRoot, 'release-artifacts');
  for (const [directory, bundlePath] of [
    [platformDirectories.linux, 'bundle/deb/disksage.deb'],
    [platformDirectories.linux, 'bundle/appimage/disksage.AppImage'],
    [platformDirectories.windows, 'bundle/msi/disksage.msi'],
    [platformDirectories.windows, 'bundle/nsis/disksage-setup.exe'],
    [platformDirectories.macos, 'bundle/dmg/disksage.dmg'],
  ] as const) {
    const absolutePath = join(artifactRoot, directory, bundlePath);
    mkdirSync(dirname(absolutePath), { recursive: true });
    writeFileSync(absolutePath, `bundle:${bundlePath}`);
  }
  for (const assetName of operationalAssetNames) {
    const platformDirectory = assetName.includes('windows')
      ? platformDirectories.windows
      : assetName.includes('macos')
        ? platformDirectories.macos
        : platformDirectories.linux;
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

/** Execute the exact shared release admission boundary in one fixture. */
function runReleaseArtifactVerifier(fixtureRoot: string) {
  return spawnSync(
    'bash',
    [
      resolve(repositoryRoot, '.github/scripts/verify-release-artifacts.sh'),
      join(fixtureRoot, 'release-artifacts'),
      runAttempt,
    ],
    { cwd: repositoryRoot, encoding: 'utf8' },
  );
}

describe('release artifact provenance contract', () => {
  it('separates build, attestation, and publication authority on exact artifacts', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    const verifier = readRepositoryFile('.github/scripts/verify-release-artifacts.sh');
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
      'bash .github/scripts/verify-release-artifacts.sh release-artifacts "${{ github.run_id }}"',
    );
    expect(attestJob).toContain(
      'actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6',
    );
    expect(attestJob).toContain('subject-path: release-artifacts/**/*');
    expect(attestJob).toContain('Generate and validate source-bound SBOM');
    expect(attestJob).toContain('disksage.spdx.json');
    expect(attestJob).not.toContain('require_exactly_one_file');
    expect(verifier).toContain('expected exactly 17 regular files');
    expect(verifier).toContain('require_exactly_one_file "${expected_dirs[0]}" disksage-cloud-plan-linux-x86_64');

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
        const linuxDirectory = join(
          fixtureRoot,
          'release-artifacts',
          platformDirectories.linux,
        );
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
    'rejects one required CLI duplicated into another platform namespace',
    () => {
      const fixtureRoot = createReleaseArtifactFixture();
      try {
        const duplicatedName = 'disksage-cloud-plan-linux-x86_64';
        const sourcePath = join(
          fixtureRoot,
          'release-artifacts',
          platformDirectories.linux,
          duplicatedName,
        );
        const duplicatePath = join(
          fixtureRoot,
          'release-artifacts',
          platformDirectories.windows,
          duplicatedName,
        );
        mkdirSync(dirname(duplicatePath), { recursive: true });
        writeFileSync(duplicatePath, readFileSync(sourcePath));

        const result = runReleaseArtifactVerifier(fixtureRoot);
        expect(result.status).not.toBe(0);
        expect(result.stderr).toContain('Unexpected release artifact entries');
        expect(result.stderr).toContain('expected exactly 17 regular files, found 18');
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
