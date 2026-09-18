import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');

describe('final source-object mutation contract', () => {
  it('does not authorize the final staging move from the reviewed pathname alone', () => {
    expect(
      safetySource,
      'a revalidated pathname is evidence, not authority for the final source-object mutation',
    ).not.toContain('std::fs::rename(path, &staged)');
  });

  it('does not submit a private staging pathname as native Trash restore authority', () => {
    expect(
      safetySource,
      'native Trash metadata must preserve the user-reviewed original location rather than a private DiskSage staging path',
    ).not.toContain('platform_trash_delete(&staged)');
  });

  it('proves pathname substitution cannot redirect the final reversible Trash mutation', () => {
    expect(
      safetySource,
      'the safety owner must exercise a real filesystem source substitution at the final staging boundary and prove the replacement object is never mutated',
    ).toContain('fn final_trash_source_substitution_does_not_mutate_replacement()');
  });

  it('proves pathname substitution cannot redirect the final permanent staging mutation', () => {
    expect(
      safetySource,
      'the safety owner must exercise the same final source-object invariant for the permanent staging path without widening irreversible deletion authority',
    ).toContain('fn final_permanent_source_substitution_does_not_mutate_replacement()');
  });
});
