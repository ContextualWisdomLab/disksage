import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const source = readFileSync('src-tauri/src/bin/disksage-cargo-target-clean.rs', 'utf8');

function functionBody(name: string): string {
  const marker = `fn ${name}`;
  const start = source.indexOf(marker);
  expect(start, `${name} must exist`).toBeGreaterThanOrEqual(0);
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

describe('cargo target clean help contract', () => {
  it('treats a sole help flag as terminal and rejects help mixed with runtime input', () => {
    const parser = functionBody('parse_args');
    const entry = functionBody('main');

    expect(parser).toContain('ParseOutcome::Help');
    expect(parser).toContain('help-cannot-be-combined-with-runtime-input');
    expect(parser).toMatch(
      /args\.len\(\)\s*==\s*1\s*&&\s*\(args\[0\]\s*==\s*"-h"\s*\|\|\s*args\[0\]\s*==\s*"--help"\)/,
    );
    expect(parser).not.toContain('clean_cargo_target');
    expect(entry).toContain('parse_args');
    const helpArm = entry.indexOf('ParseOutcome::Help');
    const runOutcome = entry.indexOf('Ok(ParseOutcome::Run');
    const runArm = entry.indexOf('clean_cargo_target');
    expect(helpArm).toBeGreaterThanOrEqual(0);
    expect(runOutcome).toBeGreaterThan(helpArm);
    expect(runArm).toBeGreaterThan(runOutcome);
    const helpBody = entry.slice(helpArm, runOutcome);
    expect(helpBody).toContain('ExitCode::SUCCESS');
    expect(helpBody).not.toContain('clean_cargo_target');
  });
});
