import { createHash } from 'node:crypto';
import {
  mkdirSync,
  mkdtempSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
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

/** Write fixture bytes while creating any required parent directory. */
function write(path: string, bytes: Buffer | string) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, bytes);
}

/** Materialize one operational CLI and the checksum record that owns that adjacent file. */
function addCli(root: string, dir: string, name: string) {
  const bytes = Buffer.from(`cli:${name}`);
  const path = join(root, dir, name);
  write(path, bytes);
  write(`${path}.sha256`, `${createHash('sha256').update(bytes).digest('hex')}  ${name}\n`);
}

/** Build the exact 17-file release tree emitted by the three-platform release matrix. */
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

/** Execute the production release verifier against an isolated artifact tree. */
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

/** Provide an exact release fixture and remove it regardless of assertion outcome. */
function withFixture(assertion: (artifactRoot: string) => void) {
  const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-windows-namespace-'));
  const artifactRoot = join(fixtureRoot, 'release-artifacts');
  try {
    materializeReleaseArtifacts(artifactRoot);
    assertion(artifactRoot);
  } finally {
    rmSync(fixtureRoot, { recursive: true, force: true });
  }
}

describe('release artifact Windows namespace regression', () => {
  it.runIf(process.platform !== 'win32')(
    'accepts the exact windows-2022 directory emitted by the release matrix',
    () => withFixture((artifactRoot) => {
      const result = verify(artifactRoot);
      expect(result.status, result.stderr).toBe(0);
      expect(result.stderr).toBe('');
    }),
  );

  it.runIf(process.platform !== 'win32')(
    'rejects a Windows bundle moved into another platform namespace',
    () => withFixture((artifactRoot) => {
      const source = join(artifactRoot, dirs.windows, 'bundle/msi/disksage.msi');
      const escaped = join(artifactRoot, dirs.linux, 'bundle/msi/disksage.msi');
      mkdirSync(dirname(escaped), { recursive: true });
      renameSync(source, escaped);

      const result = verify(artifactRoot);
      expect(result.status).not.toBe(0);
      expect(result.stderr).toContain('Windows MSI bundle');
    }),
  );

  it.runIf(process.platform !== 'win32')(
    'rejects an empty artifact even when the exact file count is preserved',
    () => withFixture((artifactRoot) => {
      write(join(artifactRoot, dirs.linux, 'bundle/deb/disksage.deb'), '');

      const result = verify(artifactRoot);
      expect(result.status).not.toBe(0);
      expect(result.stderr).toContain('Empty release artifact');
    }),
  );

  it.runIf(process.platform !== 'win32')(
    'rejects a non-regular entry before provenance admission',
    () => withFixture((artifactRoot) => {
      symlinkSync(
        'disksage-cloud-plan-linux-x86_64',
        join(artifactRoot, dirs.linux, 'unexpected-cli-alias'),
      );

      const result = verify(artifactRoot);
      expect(result.status).not.toBe(0);
      expect(result.stderr).toContain('non-regular path');
    }),
  );

  it.runIf(process.platform !== 'win32')(
    'rejects an unexpected regular file through the exact 17-file invariant',
    () => withFixture((artifactRoot) => {
      write(join(artifactRoot, dirs.linux, 'unexpected-release-note.txt'), 'unexpected');

      const result = verify(artifactRoot);
      expect(result.status).not.toBe(0);
      expect(result.stderr).toContain('expected exactly 17 regular files, found 18');
    }),
  );

  it.runIf(process.platform !== 'win32')(
    'rejects a checksum record that claims ownership of another adjacent CLI',
    () => withFixture((artifactRoot) => {
      const otherName = 'disksage-duplicate-audit-linux-x86_64';
      const otherBytes = Buffer.from(`cli:${otherName}`);
      write(
        join(artifactRoot, dirs.linux, 'disksage-cloud-plan-linux-x86_64.sha256'),
        `${createHash('sha256').update(otherBytes).digest('hex')}  ${otherName}\n`,
      );

      const result = verify(artifactRoot);
      expect(result.status).not.toBe(0);
      expect(result.stderr).toContain(
        'must reference its adjacent operational CLI disksage-cloud-plan-linux-x86_64 exactly once',
      );
    }),
  );
});
