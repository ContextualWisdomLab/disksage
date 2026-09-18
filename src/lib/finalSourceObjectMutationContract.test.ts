import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');
const linuxFinalMutationFixture = readFileSync(
  'src-tauri/src/linux_final_mutation_fail_closed_tests.rs',
  'utf8',
);
const tauriLibSource = readFileSync('src-tauri/src/lib.rs', 'utf8');

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

  it('requires the wired Linux filesystem fixture to fail closed before reversible mutation', () => {
    expect(
      tauriLibSource,
      'the Linux final-mutation acceptance module must be part of the Rust test graph',
    ).toContain('mod linux_final_mutation_fail_closed_tests;');
    expect(
      linuxFinalMutationFixture,
      'Linux acceptance must exercise the production Trash boundary and prove mutation never starts',
    ).toContain('fn final_trash_source_substitution_never_becomes_mutation_subject()');
  });

  it('requires the wired Linux filesystem fixture to fail closed before permanent mutation', () => {
    expect(
      tauriLibSource,
      'the Linux final-mutation acceptance module must be part of the Rust test graph',
    ).toContain('mod linux_final_mutation_fail_closed_tests;');
    expect(
      linuxFinalMutationFixture,
      'Linux permanent-delete acceptance must call the production boundary and prove mutation never starts',
    ).toContain('fn final_permanent_source_substitution_never_becomes_mutation_subject()');
  });
});
