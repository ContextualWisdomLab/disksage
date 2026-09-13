import { createHash } from 'node:crypto';
import {
  mkdirSync,
  mkdtempSync,
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

/** Create one complete 17-file release artifact tree accepted by the canonical verifier. */
function createCompleteReleaseFixture(): string {
  const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-allowlist-'));
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

/** Execute the one source-controlled release admission boundary against one fixture. */
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
