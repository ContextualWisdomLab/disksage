import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const workflow = readFileSync(
  new URL('../../.github/workflows/test.yml', import.meta.url),
  'utf8',
);

describe('bounded Rust coverage command diagnostic', () => {
  it('line-bounds oversized compiler invocations before edge truncation', () => {
    const lineLimit = workflow.indexOf(
      'MAX_COVERAGE_COMMAND_DIAGNOSTIC_LINE_BYTES=2048',
    );
    const lineBound = workflow.indexOf(
      'LC_ALL=C awk -v max_bytes="$MAX_COVERAGE_COMMAND_DIAGNOSTIC_LINE_BYTES"',
    );
    const boundedInput = workflow.indexOf(
      'coverage_command_bytes="$(wc -c < coverage-command.line-bounded.log | tr -d \' \')"',
    );
    const head = workflow.indexOf(
      'head -c "$COVERAGE_COMMAND_DIAGNOSTIC_EDGE_BYTES" coverage-command.line-bounded.log',
    );
    const tail = workflow.indexOf(
      'tail -c "$COVERAGE_COMMAND_DIAGNOSTIC_EDGE_BYTES" coverage-command.line-bounded.log',
    );

    expect(lineLimit).toBeGreaterThan(-1);
    expect(lineBound).toBeGreaterThan(lineLimit);
    expect(boundedInput).toBeGreaterThan(lineBound);
    expect(head).toBeGreaterThan(boundedInput);
    expect(tail).toBeGreaterThan(head);
  });

  it('deletes the intermediate line-bounded log after sanitizing the diagnostic', () => {
    expect(workflow).toContain(
      'rm -f coverage-command.raw.log coverage-command.line-bounded.log coverage-command.bounded.log',
    );
  });
});
