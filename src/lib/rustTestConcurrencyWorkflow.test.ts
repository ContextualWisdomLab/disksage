import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const workflow = readFileSync(
  new URL('../../.github/workflows/test.yml', import.meta.url),
  'utf8',
);

const jobBody = (name: string, nextName: string): string => {
  const start = `  ${name}:\n`;
  const end = `\n  ${nextName}:\n`;
  const startIndex = workflow.indexOf(start);
  const endIndex = workflow.indexOf(end, startIndex + start.length);
  expect(startIndex, `${name} job must exist`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `${nextName} job must follow ${name}`).toBeGreaterThan(startIndex);
  return workflow.slice(startIndex, endIndex);
};

describe('native Rust test concurrency contract', () => {
  it('serializes host-global active-use evidence probes in every test-executing Rust job', () => {
    const nativeTestJob = jobBody('test', 'windows-home-resolution');
    const coverageJob = jobBody('coverage-evidence', 'llm-engine-build');

    expect(nativeTestJob).toContain('RUST_TEST_THREADS: "1"');
    expect(coverageJob).toContain('RUST_TEST_THREADS: "1"');
    expect(workflow.split('RUST_TEST_THREADS: "1"').length - 1).toBe(2);
  });

  it('does not restore compiled target artifacts into exact-head Rust evidence jobs', () => {
    const rustCacheUses = workflow.split('uses: Swatinem/rust-cache@').length - 1;
    const registryOnlyCaches = workflow.split('cache-targets: false').length - 1;

    expect(rustCacheUses).toBeGreaterThan(0);
    expect(registryOnlyCaches).toBe(rustCacheUses);
  });

  it('keeps exact-head identity and exact coverage thresholds unchanged while serializing tests', () => {
    expect(workflow).toContain(
      'HEAD_SHA: ${{ github.event.pull_request.head.sha || github.sha }}',
    );
    expect(workflow).toContain(
      'cargo llvm-cov --locked --no-cfg-coverage --no-cfg-coverage-nightly --all-features --manifest-path src-tauri/Cargo.toml --branch --json --output-path coverage.json',
    );
    expect(workflow).toContain('value.percent !== 100');
  });
});
