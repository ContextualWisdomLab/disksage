import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it, vi } from 'vitest';
import {
  main,
  readCargoPackageVersion,
  readJsonVersion,
  validateReleaseVersion,
  verifyReleaseVersion,
} from '../../scripts/ci/release-version.mjs';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/** Read one UTF-8 file from the source-controlled repository root. */
function readRepositoryFile(relativePath: string): string {
  return readFileSync(resolve(repositoryRoot, relativePath), 'utf8');
}

describe('release version contract', () => {
  it('binds Tauri packaging to the cross-platform version gate', () => {
    const packageManifest = JSON.parse(readRepositoryFile('package.json')) as {
      scripts: Record<string, string>;
    };
    const tauriConfig = JSON.parse(
      readRepositoryFile('src-tauri/tauri.conf.json'),
    ) as { build: { beforeBuildCommand: string } };

    expect(packageManifest.scripts['verify:release-version']).toContain(
      "import('./scripts/ci/release-version.mjs')",
    );
    expect(packageManifest.scripts.build).toBe(
      'npm run verify:release-version && vite build',
    );
    expect(tauriConfig.build.beforeBuildCommand).toBe('npm run build');
  });

  it('reads valid JSON and Cargo package versions', () => {
    expect(readJsonVersion('package.json', () => '{"version":"1.2.3"}')).toBe('1.2.3');
    expect(
      readCargoPackageVersion(
        'Cargo.toml',
        () => '# preamble\n[workspace]\nmembers = []\n\n[package]\nname = "disksage"\nversion = "1.2.3" # buyer-visible\nedition = "2021"\n\n[dependencies]\n',
      ),
    ).toBe('1.2.3');
  });

  it('refuses invalid, missing, empty, null, or ambiguous manifest versions', () => {
    expect(() => readJsonVersion('broken.json', () => '{')).toThrow(
      'Release manifest broken.json is missing or invalid JSON.',
    );
    expect(() => readJsonVersion('missing.json', () => '{}')).toThrow(
      'Release manifest missing.json must define one non-empty string version.',
    );
    expect(() => readJsonVersion('empty.json', () => '{"version":""}')).toThrow(
      'Release manifest empty.json must define one non-empty string version.',
    );
    expect(() => readJsonVersion('null.json', () => 'null')).toThrow(
      'Release manifest null.json must define one non-empty string version.',
    );
    expect(() => readCargoPackageVersion('missing.toml', () => '[workspace]\nmembers = []\n')).toThrow(
      'Release manifest missing.toml must define exactly one package version.',
    );
    expect(() => readCargoPackageVersion('duplicate.toml', () => '[package]\nversion = "1.0.0"\nversion = "1.0.1"\n')).toThrow(
      'Release manifest duplicate.toml must define exactly one package version.',
    );
  });

  it('accepts matching manifests for branches and exact release tags', () => {
    expect(validateReleaseVersion({
      packageVersion: '1.2.3-beta.1+build.7',
      cargoVersion: '1.2.3-beta.1+build.7',
      tauriVersion: '1.2.3-beta.1+build.7',
    })).toBe('Release version contract passed for 1.2.3-beta.1+build.7.');
    expect(validateReleaseVersion({
      packageVersion: '0.1.0',
      cargoVersion: '0.1.0',
      tauriVersion: '0.1.0',
      githubRef: 'refs/tags/v0.1.0',
      githubRefName: 'v0.1.0',
    })).toBe('Release version contract passed for 0.1.0.');
  });

  it('refuses manifest disagreement, malformed SemVer, and tag drift', () => {
    expect(() => validateReleaseVersion({
      packageVersion: '0.1.0', cargoVersion: '0.2.0', tauriVersion: '0.1.0',
    })).toThrow('Release manifest versions disagree: package.json=0.1.0, Cargo.toml=0.2.0, tauri.conf.json=0.1.0.');
    expect(() => validateReleaseVersion({
      packageVersion: '0.1.0', cargoVersion: '0.1.0', tauriVersion: '0.2.0',
    })).toThrow('Release manifest versions disagree: package.json=0.1.0, Cargo.toml=0.1.0, tauri.conf.json=0.2.0.');
    for (const invalidVersion of ['01.0.0', '1.0.0-01', '1.0.0-alpha.01']) {
      expect(() => validateReleaseVersion({
        packageVersion: invalidVersion,
        cargoVersion: invalidVersion,
        tauriVersion: invalidVersion,
      })).toThrow(`Release manifest version ${invalidVersion} is not valid Semantic Versioning.`);
    }
    expect(() => validateReleaseVersion({
      packageVersion: '0.1.0', cargoVersion: '0.1.0', tauriVersion: '0.1.0',
      githubRef: 'refs/tags/v0.2.0', githubRefName: 'v0.2.0',
    })).toThrow('Release tag v0.2.0 does not match manifest version v0.1.0.');
  });

  it('loads repository manifests through injectable runtime boundaries', () => {
    const manifests = new Map([
      ['/fixture/package.json', '{"version":"0.1.0"}'],
      ['/fixture/src-tauri/Cargo.toml', '[package]\nname = "disksage"\nversion = "0.1.0"\n[dependencies]\n'],
      ['/fixture/src-tauri/tauri.conf.json', '{"version":"0.1.0"}'],
    ]);
    const readText = vi.fn((path: string) => {
      const value = manifests.get(path);
      if (value === undefined) throw new Error(`unexpected path ${path}`);
      return value;
    });
    expect(verifyReleaseVersion({
      repositoryRoot: '/fixture',
      environment: { GITHUB_REF: 'refs/tags/v0.1.0', GITHUB_REF_NAME: 'v0.1.0' },
      readText,
    })).toBe('Release version contract passed for 0.1.0.');
    expect(readText).toHaveBeenCalledTimes(3);
    expect(verifyReleaseVersion()).toBe('Release version contract passed for 0.1.0.');
  });

  it('reports stable success and failure outcomes at the CLI boundary', () => {
    const output: string[] = [];
    const errors: string[] = [];
    const exitCodes: number[] = [];
    expect(main({
      verify: () => 'passed',
      writeOutput: (message: string) => output.push(message),
      writeError: (message: string) => errors.push(message),
      setExitCode: (code: number) => exitCodes.push(code),
    })).toBe(true);
    expect(output).toEqual(['passed']);
    expect(main({
      verify: () => { throw new Error('failed'); },
      writeOutput: (message: string) => output.push(message),
      writeError: (message: string) => errors.push(message),
      setExitCode: (code: number) => exitCodes.push(code),
    })).toBe(false);
    expect(main({
      verify: () => { throw 'non-error'; },
      writeOutput: (message: string) => output.push(message),
      writeError: (message: string) => errors.push(message),
      setExitCode: (code: number) => exitCodes.push(code),
    })).toBe(false);
    expect(errors).toEqual(['failed', 'Unknown release version failure.']);
    expect(exitCodes).toEqual([1, 1]);
  });
});
