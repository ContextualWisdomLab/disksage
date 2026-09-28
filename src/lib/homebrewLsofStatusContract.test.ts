import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const source = readFileSync('src-tauri/src/homebrew_audit.rs', 'utf8');

function functionBody(name: string): string {
  const marker = `fn ${name}`;
  const start = source.indexOf(marker);
  expect(start, `${name} must exist as a private Homebrew audit helper`).toBeGreaterThanOrEqual(0);

  const brace = source.indexOf('{', start);
  expect(brace).toBeGreaterThan(start);

  let depth = 0;
  for (let index = brace; index < source.length; index += 1) {
    if (source[index] === '{') depth += 1;
    if (source[index] === '}') {
      depth -= 1;
      if (depth === 0) return source.slice(start, index + 1);
    }
  }
  throw new Error(`unterminated Rust function ${name}`);
}

describe('Homebrew lsof completion evidence contract', () => {
  it('classifies status/stdout/stderr before running PID parsing', () => {
    const helper = functionBody('classify_lsof_result');
    const probe = functionBody('running_pids_under_prefix');

    expect(helper).toContain('exit_code');
    expect(helper).toContain('stdout');
    expect(helper).toContain('stderr');
    expect(probe).toContain('classify_lsof_result');
    expect(probe).not.toContain('if code != 0 && out.is_empty()');
  });

  it('admits exit 1 as no-match only when both output streams are empty', () => {
    const helper = functionBody('classify_lsof_result');

    expect(helper).toMatch(
      /exit_code\s*==\s*1\s*&&\s*stdout\.is_empty\(\)\s*&&\s*stderr\.is_empty\(\)/,
    );
    expect(helper).toContain('active-use-probe-failed');
  });

  it('retains non-zero status and stderr before any PID parsing', () => {
    const helper = functionBody('classify_lsof_result');
    const failureBoundary = helper.indexOf('if exit_code != 0');
    const pidParsing = helper.indexOf('for token in stdout.split');

    expect(failureBoundary).toBeGreaterThanOrEqual(0);
    expect(pidParsing).toBeGreaterThan(failureBoundary);
    expect(helper).toContain('lsof-exit-status:{exit_code}:stderr:{stderr}');
  });
});
