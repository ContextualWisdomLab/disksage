import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, renameSync, rmSync, writeFileSync } from 'node:fs';
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

/** Writes one fixture file, creating only the parent directories required by that fixture. */
function write(path: string, bytes: Buffer | string) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, bytes);
}

/** Adds one operational CLI fixture together with its adjacent SHA-256 receipt. */
function addCli(artifactRoot: string, directory: string, name: string) {
  const bytes = Buffer.from(`cli:${name}`);
  const assetPath = join(artifactRoot, directory, name);
  write(assetPath, bytes);
  write(
    `${assetPath}.sha256`,
    `${createHash('sha256').update(bytes).digest('hex')}  ${name}\n`,
  );
}

/** Materializes the exact 17-file Linux, Windows, and macOS release artifact contract. */
function materializeExactArtifactSet(artifactRoot: string) {
  write(join(artifactRoot, platformDirectories.linux, 'bundle/deb/disksage.deb'), 'deb');
  write(join(artifactRoot, platformDirectories.linux, 'bundle/appimage/disksage.AppImage'), 'appimage');
  write(join(artifactRoot, platformDirectories.windows, 'bundle/msi/disksage.msi'), 'msi');
  write(join(artifactRoot, platformDirectories.windows, 'bundle/nsis/disksage-setup.exe'), 'nsis');
  write(join(artifactRoot, platformDirectories.macos, 'bundle/dmg/disksage.dmg'), 'dmg');

  addCli(artifactRoot, platformDirectories.linux, 'disksage-cloud-plan-linux-x86_64');
  addCli(artifactRoot, platformDirectories.linux, 'disksage-duplicate-audit-linux-x86_64');
  addCli(artifactRoot, platformDirectories.windows, 'disksage-cloud-plan-windows-x86_64.exe');
  addCli(artifactRoot, platformDirectories.windows, 'disksage-duplicate-audit-windows-x86_64.exe');
  addCli(artifactRoot, platformDirectories.macos, 'disksage-cloud-plan-macos-arm64');
  addCli(artifactRoot, platformDirectories.macos, 'disksage-duplicate-audit-macos-arm64');
}

/** Runs the repository-owned verifier against one isolated downloaded-artifact fixture. */
function verify(artifactRoot: string) {
  return spawnSync(
    'bash',
    [
      resolve(repositoryRoot, '.github/scripts/verify-release-artifacts.sh'),
      artifactRoot,
      runAttempt,
    ],
    { cwd: repositoryRoot, encoding: 'utf8' },
  );
}

describe('release artifact verifier directory contract', () => {
  it.runIf(process.platform !== 'win32')(
    'accepts the exact platform namespaces uploaded by the release matrix',
    () => {
      const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-artifact-verifier-'));
      const artifactRoot = join(fixtureRoot, 'release-artifacts');
      try {
        materializeExactArtifactSet(artifactRoot);

        const result = verify(artifactRoot);

        expect(result.status, result.stderr).toBe(0);
        expect(result.stderr).toBe('');
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );

  it.runIf(process.platform !== 'win32')(
    'rejects a Windows bundle that escaped its Windows artifact directory',
    () => {
      const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-artifact-verifier-'));
      const artifactRoot = join(fixtureRoot, 'release-artifacts');
      try {
        materializeExactArtifactSet(artifactRoot);
        const source = join(artifactRoot, platformDirectories.windows, 'bundle/msi/disksage.msi');
        const misplaced = join(artifactRoot, platformDirectories.linux, 'bundle/msi/disksage.msi');
        mkdirSync(dirname(misplaced), { recursive: true });
        renameSync(source, misplaced);

        const result = verify(artifactRoot);

        expect(result.status).not.toBe(0);
        expect(result.stdout).toBe('');
        expect(result.stderr).toContain('Windows MSI bundle');
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );

  it.runIf(process.platform !== 'win32')(
    'rejects a bundle nested below its exact release matrix directory',
    () => {
      const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-artifact-verifier-'));
      const artifactRoot = join(fixtureRoot, 'release-artifacts');
      try {
        materializeExactArtifactSet(artifactRoot);
        const source = join(artifactRoot, platformDirectories.linux, 'bundle/deb/disksage.deb');
        const misplaced = join(
          artifactRoot,
          platformDirectories.linux,
          'bundle/deb/unexpected/disksage.deb',
        );
        mkdirSync(dirname(misplaced), { recursive: true });
        renameSync(source, misplaced);

        const result = verify(artifactRoot);

        expect(result.status).not.toBe(0);
        expect(result.stdout).toBe('');
        expect(result.stderr).toContain('Debian bundle');
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );

  it.runIf(process.platform !== 'win32')(
    'rejects a Windows operational CLI and checksum outside the Windows artifact directory',
    () => {
      const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-artifact-verifier-'));
      const artifactRoot = join(fixtureRoot, 'release-artifacts');
      try {
        materializeExactArtifactSet(artifactRoot);
        const cliName = 'disksage-cloud-plan-windows-x86_64.exe';
        const source = join(artifactRoot, platformDirectories.windows, cliName);
        const sourceChecksum = `${source}.sha256`;
        const misplaced = join(artifactRoot, platformDirectories.linux, cliName);
        const misplacedChecksum = `${misplaced}.sha256`;
        renameSync(source, misplaced);
        renameSync(sourceChecksum, misplacedChecksum);

        const result = verify(artifactRoot);

        expect(result.status).not.toBe(0);
        expect(result.stdout).toBe('');
        expect(result.stderr).toContain(cliName);
        expect(result.stderr).toContain(platformDirectories.windows);
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );
});
