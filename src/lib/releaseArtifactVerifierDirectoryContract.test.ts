import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
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

function write(path: string, bytes: Buffer | string) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, bytes);
}

function addCli(artifactRoot: string, directory: string, name: string) {
  const bytes = Buffer.from(`cli:${name}`);
  const assetPath = join(artifactRoot, directory, name);
  write(assetPath, bytes);
  write(
    `${assetPath}.sha256`,
    `${createHash('sha256').update(bytes).digest('hex')}  ${name}\n`,
  );
}

describe('release artifact verifier directory contract', () => {
  it.runIf(process.platform !== 'win32')(
    'accepts the exact platform namespaces uploaded by the release matrix',
    () => {
      const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-artifact-verifier-'));
      const artifactRoot = join(fixtureRoot, 'release-artifacts');
      try {
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

        const result = spawnSync(
          'bash',
          [
            resolve(repositoryRoot, '.github/scripts/verify-release-artifacts.sh'),
            artifactRoot,
            runAttempt,
          ],
          { cwd: repositoryRoot, encoding: 'utf8' },
        );

        expect(result.status, result.stderr).toBe(0);
        expect(result.stderr).toBe('');
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );
});
