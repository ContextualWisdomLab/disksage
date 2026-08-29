import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const verifierPath = resolve(repositoryRoot, '.github/scripts/verify-release-artifacts.sh');

function writeArtifact(root: string, relativePath: string, content: string) {
  const path = join(root, relativePath);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
  return path;
}

function writeOperationalCli(root: string, namespace: string, assetName: string) {
  const bytes = `operational-cli:${assetName}`;
  writeArtifact(root, `${namespace}/${assetName}`, bytes);
  const digest = createHash('sha256').update(bytes).digest('hex');
  writeArtifact(root, `${namespace}/${assetName}.sha256`, `${digest}  ${assetName}\n`);
}

function createDownloadedReleaseFixture() {
  const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-download-'));
  const artifactRoot = join(fixtureRoot, 'release-artifacts');
  const linux = 'release-disksage-ubuntu-22.04-1';
  const windows = 'release-disksage-windows-2022-1';
  const macos = 'release-disksage-macos-latest-1';

  writeArtifact(artifactRoot, `${linux}/bundle/deb/disksage.deb`, 'deb');
  writeArtifact(artifactRoot, `${linux}/bundle/appimage/disksage.AppImage`, 'appimage');
  writeArtifact(artifactRoot, `${windows}/bundle/msi/disksage.msi`, 'msi');
  writeArtifact(artifactRoot, `${windows}/bundle/nsis/disksage-setup.exe`, 'nsis');
  writeArtifact(artifactRoot, `${macos}/bundle/dmg/disksage.dmg`, 'dmg');

  writeOperationalCli(artifactRoot, linux, 'disksage-cloud-plan-linux-x86_64');
  writeOperationalCli(artifactRoot, linux, 'disksage-duplicate-audit-linux-x86_64');
  writeOperationalCli(artifactRoot, windows, 'disksage-cloud-plan-windows-x86_64.exe');
  writeOperationalCli(artifactRoot, windows, 'disksage-duplicate-audit-windows-x86_64.exe');
  writeOperationalCli(artifactRoot, macos, 'disksage-cloud-plan-macos-arm64');
  writeOperationalCli(artifactRoot, macos, 'disksage-duplicate-audit-macos-arm64');

  return { fixtureRoot, artifactRoot };
}

describe('downloaded release artifact contract', () => {
  it.runIf(process.platform !== 'win32')(
    'accepts the exact artifact namespaces emitted by the release build matrix',
    () => {
      const { fixtureRoot, artifactRoot } = createDownloadedReleaseFixture();
      try {
        const result = spawnSync('bash', [verifierPath, artifactRoot, '1'], {
          cwd: repositoryRoot,
          encoding: 'utf8',
        });

        expect(result.status, result.stderr).toBe(0);
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );
});
