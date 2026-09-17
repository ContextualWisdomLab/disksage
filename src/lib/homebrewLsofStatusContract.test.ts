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
  it('uses one pure status/stdout/stderr interpreter before PID parsing', () => {
    const helper = functionBody('interpret_lsof_completion');
    const probe = functionBody('running_pids_under_prefix');

    expect(helper).toContain('exit_code');
    expect(helper).toContain('stdout');
    expect(helper).toContain('stderr');
    expect(probe).toContain('interpret_lsof_completion');
    expect(probe).not.toContain('if code != 0 && out.is_empty()');
  });

  it('admits exit 1 only when both output streams are empty', () => {
    const helper = functionBody('interpret_lsof_completion');

    expect(helper).toMatch(/exit_code\s*==\s*1/);
    expect(helper).toMatch(/stdout\.(?:trim\(\)|is_empty\(\))/);
    expect(helper).toMatch(/stderr\.(?:trim\(\)|is_empty\(\))/);
    expect(helper).toContain('active-use-probe-failed');
  });

  it('retains failure status and stderr diagnostics instead of returning an empty PID set', () => {
    const helper = functionBody('interpret_lsof_completion');

    expect(helper).toMatch(/exit(?:_|-)status|status|exit_code/);
    expect(helper).toContain('stderr');
    expect(helper).not.toContain('Ok(Vec::new())');
  });
});
