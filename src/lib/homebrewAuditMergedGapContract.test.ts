import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const source = readFileSync('src-tauri/src/homebrew_audit.rs', 'utf8');

describe('merged Homebrew audit safety follow-up', () => {
  it('does not treat every non-zero lsof exit with empty stdout as a successful no-match', () => {
    expect(
      source,
      'lsof exit 1 + empty stdout is the documented no-match case; other non-zero exits are incomplete active-use evidence',
    ).not.toContain('if code != 0 && out.is_empty()');
  });

  it('routes Spotlight last-use probing through the bounded subprocess boundary', () => {
    expect(
      source,
      'raw mdls Command::output bypasses the configured command timeout and must not remain in the Homebrew audit path',
    ).not.toContain('Command::new("mdls")');
  });

  it('does not retain an unbounded detached pipe-reader implementation', () => {
    expect(
      source,
      'unbounded read_to_string readers can outlive the direct child timeout and violate the bounded command contract',
    ).not.toContain('pipe.read_to_string(&mut buf)');
  });
});
