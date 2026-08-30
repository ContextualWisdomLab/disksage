import { createHash } from 'node:crypto';
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
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
  'disksage-podman-storage-repair-linux-x86_64',
  'disksage-photo-similarity-audit-linux-x86_64',
  'disksage-shared-temp-reclaim-plan-linux-x86_64',
  'disksage-cloud-plan-windows-x86_64.exe',
  'disksage-duplicate-audit-windows-x86_64.exe',
  'disksage-podman-storage-repair-windows-x86_64.exe',
  'disksage-photo-similarity-audit-windows-x86_64.exe',
  'disksage-shared-temp-reclaim-plan-windows-x86_64.exe',
  'disksage-cloud-plan-macos-arm64',
  'disksage-duplicate-audit-macos-arm64',
  'disksage-cloud-local-eviction-batch-macos-arm64',
  'disksage-icloud-local-eviction-batch-macos-arm64',
  'disksage-cloud-local-inventory-macos-arm64',
  'disksage-onedrive-finder-verify-macos-arm64',
  'disksage-podman-storage-repair-macos-arm64',
  'disksage-photo-similarity-audit-macos-arm64',
  'disksage-shared-temp-reclaim-plan-macos-arm64',
] as const;

const platformDirectories = {
  linux: 'release-disksage-ubuntu-22.04-1',
  windows: 'release-disksage-windows-2022-1',
  macos: 'release-disksage-macos-latest-1',
} as const;

/** Read one UTF-8 file from the source-controlled repository root. */
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

/** Extract the literal Bash body from one named workflow step. */
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

/** Create one complete, valid pre-SBOM release artifact tree for verifier execution. */
function createCompleteReleaseFixture(): string {
  const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-allowlist-'));
  const artifactRoot = join(fixtureRoot, 'release-artifacts');
  const bundlePaths = [
    `${platformDirectories.linux}/bundle/deb/disksage.deb`,
    `${platformDirectories.linux}/bundle/appimage/disksage.AppImage`,
    `${platformDirectories.windows}/bundle/msi/disksage.msi`,
    `${platformDirectories.windows}/bundle/nsis/disksage-setup.exe`,
    `${platformDirectories.macos}/bundle/dmg/disksage.dmg`,
  ];
  for (const bundlePath of bundlePaths) {
    const absolutePath = join(artifactRoot, bundlePath);
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

/** Execute the source-controlled pre-SBOM release admission step against one fixture. */
function runReleaseArtifactVerifier(fixtureRoot: string) {
  const workflow = readRepositoryFile('.github/workflows/release.yml');
  const attestJob = extractWorkflowJob(workflow, 'attest-release');
  const verifier = extractWorkflowRunScript(
    attestJob,
    'Verify release artifact checksums',
  );
  return spawnSync('bash', ['-c', verifier], {
    cwd: fixtureRoot,
    encoding: 'utf8',
    env: {
      ...process.env,
      GITHUB_WORKSPACE: repositoryRoot,
      GITHUB_RUN_ATTEMPT: '1',
    },
  });
}

describe('release artifact exact-set admission', () => {
  it.runIf(process.platform !== 'win32')(
    'rejects an unexpected file that would otherwise be attested and published',
    () => {
      const fixtureRoot = createCompleteReleaseFixture();
      try {
        writeFileSync(
          join(
            fixtureRoot,
            'release-artifacts',
            platformDirectories.linux,
            'unexpected-debug-dump.txt',
          ),
          'buyer-private-or-unreviewed-output',
        );

        const result = runReleaseArtifactVerifier(fixtureRoot);

        expect(result.status).not.toBe(0);
        expect(result.stderr).toContain('Unexpected release artifact entries');
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );

  it.runIf(process.platform !== 'win32')(
    'rejects a symlink before it can become a provenance subject',
    () => {
      const fixtureRoot = createCompleteReleaseFixture();
      try {
        symlinkSync(
          'disksage-cloud-plan-linux-x86_64',
          join(
            fixtureRoot,
            'release-artifacts',
            platformDirectories.linux,
            'unexpected-cli-alias',
          ),
        );

        const result = runReleaseArtifactVerifier(fixtureRoot);

        expect(result.status).not.toBe(0);
        expect(result.stderr).toContain('non-regular path');
        expect(result.stderr).toContain('unexpected-cli-alias');
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );
});
