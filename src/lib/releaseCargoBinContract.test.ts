import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

function readRepositoryFile(relativePath: string): string {
  return readFileSync(resolve(repositoryRoot, relativePath), 'utf8');
}

function requiredCargoBinFeatures(cargoToml: string): Set<string> {
  const required = new Set<string>();
  const binBlocks = cargoToml.split('[[bin]]').slice(1);
  for (const rawBlock of binBlocks) {
    const block = rawBlock.split(/\n\[(?!\[)/, 1)[0] ?? '';
    const match = block.match(/^required-features\s*=\s*\[([^\]]*)\]/m);
    if (!match) continue;
    for (const quoted of match[1].matchAll(/"([^"]+)"/g)) {
      required.add(quoted[1]);
    }
  }
  return required;
}

function tauriReleaseFeatures(releaseWorkflow: string): Set<string> {
  const buildStep = releaseWorkflow.match(
    /- name: Tauri build \(with embedded LLM\)[\s\S]*?\n\s+run:\s+([^\n]+)/,
  );
  if (!buildStep) throw new Error('Missing Tauri release build step');
  const features = buildStep[1].match(/--features\s+([^\s]+)/);
  if (!features) throw new Error('Tauri release build does not declare Cargo features');
  return new Set(features[1].split(',').map((feature) => feature.trim()).filter(Boolean));
}

describe('release Cargo binary bundle contract', () => {
  it('keeps CLI-only Cargo binaries out of the Tauri bundle and stages publishable ones separately', () => {
    const cargoToml = readRepositoryFile('src-tauri/Cargo.toml');
    const releaseWorkflow = readRepositoryFile('.github/workflows/release.yml');
    const requiredFeatures = [...requiredCargoBinFeatures(cargoToml)].sort();
    const releaseFeatures = tauriReleaseFeatures(releaseWorkflow);

    expect(requiredFeatures.length).toBeGreaterThan(0);
    expect(releaseFeatures.has('llm-engine')).toBe(true);
    expect(requiredFeatures.filter((feature) => releaseFeatures.has(feature))).toEqual([]);
    expect(releaseWorkflow).toContain(
      'cargo build --manifest-path src-tauri/Cargo.toml --release --features cloud-cli',
    );
    expect(releaseWorkflow).toContain('--bin disksage-cloud-plan --bin disksage-duplicate-audit');
  });
});
