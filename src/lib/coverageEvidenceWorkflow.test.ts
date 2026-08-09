import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const workflow = readFileSync(
  new URL('../../.github/workflows/test.yml', import.meta.url),
  'utf8',
);

describe('Test workflow coverage evidence contract', () => {
  it('binds coverage evidence to the exact pull-request head', () => {
    expect(workflow).toContain(
      'ref: ${{ github.event.pull_request.head.sha || github.sha }}',
    );
    expect(workflow).toContain(
      'HEAD_SHA: ${{ github.event.pull_request.head.sha || github.sha }}',
    );
  });

  it('measures Rust branch coverage instead of synthesizing percentages', () => {
    expect(workflow).toContain('tool: cargo-llvm-cov');
    expect(workflow).toContain(
      'cargo llvm-cov --manifest-path src-tauri/Cargo.toml --branch --json --summary-only --output-path coverage.json',
    );
    expect(workflow).toContain('coverage.json');
    expect(workflow).toContain('coverage-evidence.json');
  });

  it('measures the production Rust graph instead of cfg-pruned substitutes', () => {
    expect(workflow).toContain('--no-cfg-coverage');
    expect(workflow).toContain('--no-cfg-coverage-nightly');
  });

  it('preserves bounded exact-head metric diagnostics when the 100% gate fails', () => {
    expect(workflow).toContain('coverage-diagnostic.json');
    expect(workflow).toContain('name: coverage-diagnostic-${{ env.HEAD_SHA }}');
    expect(workflow).toContain('path: coverage-diagnostic.json');
    expect(workflow).toContain('if: always()');
    expect(workflow).toContain('regions: totals?.regions ?? null');
    expect(workflow).toContain('branches: totals?.branches ?? null');
    expect(workflow).toContain('functions: totals?.functions ?? null');
    expect(workflow).toContain('lines: totals?.lines ?? null');
  });

  it('uploads fail-closed evidence under the organization contract name', () => {
    expect(workflow).toContain('name: coverage-evidence');
    expect(workflow).toContain('path: coverage-evidence.json');
    expect(workflow).toContain('if-no-files-found: error');
    expect(workflow).toContain('statement_coverage');
    expect(workflow).toContain('branch_coverage');
    expect(workflow).toContain('function_coverage');
    expect(workflow).toContain('line_coverage');
  });
});