import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(import.meta.dirname, '../..');
const runAttempt = '1';
const dirs = {
  linux: `release-disksage-ubuntu-22.04-${runAttempt}`,
  windows: `release-disksage-windows-2022-${runAttempt}`,
  macos: `release-disksage-macos-latest-${runAttempt}`,
} as const;

function write(path: string, bytes: Buffer | string) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, bytes);
}

function addCli(root: string, dir: string, name: string) {
  const bytes = Buffer.from(`cli:${name}`);
  const path = join(root, dir, name);
  write(path, bytes);
  write(`${path}.sha256`, `${createHash('sha256').update(bytes).digest('hex')}  ${name}\n`);
}

function materializeReleaseArtifacts(root: string) {
  write(join(root, dirs.linux, 'bundle/deb/disksage.deb'), 'deb');
  write(join(root, dirs.linux, 'bundle/appimage/disksage.AppImage'), 'appimage');
  write(join(root, dirs.windows, 'bundle/msi/disksage.msi'), 'msi');
  write(join(root, dirs.windows, 'bundle/nsis/disksage-setup.exe'), 'nsis');
  write(join(root, dirs.macos, 'bundle/dmg/disksage.dmg'), 'dmg');

  addCli(root, dirs.linux, 'disksage-cloud-plan-linux-x86_64');
  addCli(root, dirs.linux, 'disksage-duplicate-audit-linux-x86_64');
  addCli(root, dirs.windows, 'disksage-cloud-plan-windows-x86_64.exe');
  addCli(root, dirs.windows, 'disksage-duplicate-audit-windows-x86_64.exe');
  addCli(root, dirs.macos, 'disksage-cloud-plan-macos-arm64');
  addCli(root, dirs.macos, 'disksage-duplicate-audit-macos-arm64');
}

describe('release artifact Windows namespace regression', () => {
  it.runIf(process.platform !== 'win32')(
    'accepts the exact windows-2022 directory emitted by the release matrix',
    () => {
      const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-windows-namespace-'));
      const artifactRoot = join(fixtureRoot, 'release-artifacts');
      try {
        materializeReleaseArtifacts(artifactRoot);
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
