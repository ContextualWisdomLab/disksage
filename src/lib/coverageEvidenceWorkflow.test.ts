import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const workflow = readFileSync(
  new URL('../../.github/workflows/test.yml', import.meta.url),
  'utf8',
);
const diagnosticHelper = readFileSync(
  new URL('../../.github/scripts/bound-coverage-command-diagnostic.sh', import.meta.url),
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
    expect(workflow.split(exactHeadCheckout).length - 1).toBe(4);
  });

  it('requires every Rust test and coverage invocation to honor the committed lockfile', () => {
    const rustExecutionLines = workflow
      .split('\n')
      .map((line) => line.trim())
      .filter(
        (line) =>
          line.startsWith('cargo test ') || line.startsWith('cargo llvm-cov '),
      );

    expect(rustExecutionLines.length).toBeGreaterThan(0);
    for (const line of rustExecutionLines) {
      expect(line, `unlocked Rust CI command: ${line}`).toContain('--locked');
    }
  });

  it('measures Rust branch coverage instead of synthesizing percentages', () => {
    expect(workflow).toContain('tool: cargo-llvm-cov');
    expect(workflow).toContain(
      'cargo llvm-cov --locked --no-cfg-coverage --no-cfg-coverage-nightly --all-features --manifest-path src-tauri/Cargo.toml --branch --json --output-path coverage.json',
    );
    expect(workflow).not.toContain('--summary-only');
    expect(workflow).toContain('coverage.json');
    expect(workflow).toContain('coverage-evidence.json');
  });

  it('keeps coverage instrumentation from changing production cfg semantics', () => {
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

  it('preserves both ends of a bounded sanitized command diagnostic when measurement itself fails', () => {
    expect(workflow).toContain('id: rust-coverage');
    expect(workflow).toContain('coverage-command.raw.log');
    expect(workflow).toContain('coverage-command.bounded.log');
    expect(workflow).toContain('coverage-command-diagnostic.log');
    expect(diagnosticHelper).toContain('max_total_bytes=32768');
    expect(diagnosticHelper).toContain('edge_bytes=9000');
    expect(diagnosticHelper).toContain('head -c "$edge_bytes" "$line_bounded_log"');
    expect(diagnosticHelper).toContain('tail -c "$edge_bytes" "$line_bounded_log"');
    expect(workflow).toContain("replaceAll(workspace, '<repo>')");
    expect(workflow).toContain("replaceAll(home, '<home>')");
    expect(workflow).toContain(
      "if: failure() && steps.rust-coverage.outcome == 'failure'",
    );
  });

  it('preserves the authoritative coverage exit status when diagnostic rendering fails', () => {
    const measureStart = workflow.indexOf('name: Measure exact-head Rust coverage');
    const uploadStart = workflow.indexOf(
      'name: Upload bounded coverage command diagnostic',
      measureStart,
    );
    expect(measureStart).toBeGreaterThanOrEqual(0);
    expect(uploadStart).toBeGreaterThan(measureStart);
    const measureStep = workflow.slice(measureStart, uploadStart);
    expect(measureStep).toContain('coverage_status=$?');
    expect(measureStep).toContain('diagnostic_status=$?');
    expect(measureStep).toContain('redaction_status=$?');
    expect(measureStep).toContain('coverage diagnostic rendering failed');
    expect(measureStep).toContain('exit "$coverage_status"');
  });

  it('identifies the largest exact-head Rust coverage gaps without leaking runner paths', () => {
    expect(workflow).toContain('top_uncovered_files');
    expect(workflow).toContain("const marker = '/src-tauri/'");
    expect(workflow).toContain('uncovered_regions');
    expect(workflow).toContain('uncovered_branches');
    expect(workflow).toContain('uncovered_functions');
    expect(workflow).toContain('uncovered_lines');
  });

  it('preserves bounded frontend diagnostics when the all-production threshold fails', () => {
    expect(workflow).toContain('name: Build bounded frontend coverage diagnostic');
    expect(workflow).toContain("readFileSync('coverage/coverage-final.json', 'utf8')");
    expect(workflow).toContain("readFileSync('coverage/coverage-summary.json', 'utf8')");
    expect(workflow).toContain('frontend_top_uncovered_files');
    expect(workflow).toContain('frontend-coverage-diagnostic.json');
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
