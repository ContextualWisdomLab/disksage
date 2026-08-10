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

  it('binds every checkout-bearing Test job to the exact current head', () => {
    const exactHeadCheckout =
      'ref: ${{ github.event.pull_request.head.sha || github.sha }}';
    expect(workflow.split(exactHeadCheckout).length - 1).toBe(3);
  });

  it('measures Rust branch coverage instead of synthesizing percentages', () => {
    expect(workflow).toContain('tool: cargo-llvm-cov');
    expect(workflow).toContain(
      'cargo llvm-cov --manifest-path src-tauri/Cargo.toml --branch --json --output-path coverage.json',
    );
    expect(workflow).not.toContain('--summary-only');
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

  it('identifies the largest exact-head Rust coverage gaps without leaking runner paths', () => {
    expect(workflow).toContain('top_uncovered_files');
    expect(workflow).toContain("const marker = '/src-tauri/'");
    expect(workflow).toContain("return `src-tauri/${normalized.slice(markerIndex + marker.length)}`");
    expect(workflow).toContain('.slice(0, 20)');
    expect(workflow).toContain('uncovered_regions');
    expect(workflow).toContain('uncovered_branches');
    expect(workflow).toContain('uncovered_functions');
    expect(workflow).toContain('uncovered_lines');
  });

  it('preserves bounded repository-relative uncovered line numbers for test targeting', () => {
    expect(workflow).toContain('uncovered_line_numbers');
    expect(workflow).toContain('const uncoveredLineNumbers = (segments) =>');
    expect(workflow).toContain('Array.isArray(segment)');
    expect(workflow).toContain('segment[2] === 0');
    expect(workflow).toContain('.slice(0, 40)');
  });

  it('surfaces the same bounded diagnostic in logs and the GitHub step summary', () => {
    expect(workflow).toContain("console.error(`coverage-diagnostic=${JSON.stringify(diagnostic)}`)");
    expect(workflow).toContain('process.env.GITHUB_STEP_SUMMARY');
    expect(workflow).toMatch(/appendFileSync\(\s*summaryPath,/u);
    expect(workflow).toContain('Coverage diagnostic for \\`${sha}\\`');
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
