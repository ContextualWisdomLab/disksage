import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

function readRepositoryFile(relativePath: string): string {
  return readFileSync(resolve(repositoryRoot, relativePath), 'utf8').replace(/\r\n?/g, '\n');
}

describe('release Tauri binary isolation', () => {
  it('does not enable operational CLI-only features while WiX enumerates Cargo binaries', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    const cargoManifest = readRepositoryFile('src-tauri/Cargo.toml');

    expect(cargoManifest).toContain('required-features = ["volume-cli"]');
    expect(cargoManifest).toContain('required-features = ["cloud-cli"]');
    expect(cargoManifest).toContain('required-features = ["archive-cli"]');

    expect(workflow).toContain('npm run tauri -- build --features llm-engine');
    expect(workflow).not.toMatch(
      /npm run tauri -- build --features [^\n]*(?:volume-cli|cloud-cli|archive-cli)/,
    );
  });

  it('builds publishable operational CLIs explicitly after the GUI bundle', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');

    expect(workflow).toContain(
      'cargo build --manifest-path src-tauri/Cargo.toml --release --features cloud-cli',
    );
    expect(workflow).toContain('--bin disksage-cloud-plan --bin disksage-duplicate-audit');
  });

  it('registers the Windows VBScript engine before WiX MSI packaging', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    const registrationStep = 'Enable Windows Script Host for WiX MSI';
    const tauriBuildStep = 'Tauri build (with embedded LLM)';

    expect(workflow).toContain(registrationStep);
    expect(workflow).toContain("Join-Path $env:windir 'System32\\vbscript.dll'");
    expect(workflow).toContain("Join-Path $env:windir 'SysWOW64\\vbscript.dll'");
    expect(workflow).toContain("Start-Process -FilePath $regsvr32 -ArgumentList @('/s', $engine) -Wait -PassThru");
    expect(workflow).toContain("throw 'VBScript engine is unavailable on this runner.'");
    expect(workflow.indexOf(registrationStep)).toBeLessThan(workflow.indexOf(tauriBuildStep));
  });

  it('never restores a configured Cargo target directory into a release build', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    const rustCacheStep = 'uses: Swatinem/rust-cache@';
    const tauriBuildStep = 'Tauri build (with embedded LLM)';
    const cacheStart = workflow.indexOf(rustCacheStep);
    const buildStart = workflow.indexOf(tauriBuildStep);

    expect(cacheStart).toBeGreaterThanOrEqual(0);
    expect(buildStart).toBeGreaterThan(cacheStart);
    const releaseCacheBlock = workflow.slice(cacheStart, buildStart);
    expect(releaseCacheBlock).toContain('cache-targets: false');
  });
});
