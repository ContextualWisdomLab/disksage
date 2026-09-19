import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');
const linuxFinalMutationFixture = readFileSync(
  'src-tauri/src/linux_final_mutation_fail_closed_tests.rs',
  'utf8',
);
const tauriLibSource = readFileSync('src-tauri/src/lib.rs', 'utf8');

describe('final source-object mutation contract', () => {
  it('binds both identity-authorized mutation boundaries to the Linux fail-closed gate', () => {
    expect(
      safetySource,
      'Linux needs one explicit unsupported-platform boundary instead of pathname-selected mutation',
    ).toContain(
      '#[cfg(target_os = "linux")]\nfn ensure_identity_bound_final_mutation_supported()',
    );
    expect(
      safetySource.match(/ensure_identity_bound_final_mutation_supported\(\)\?/g) ?? [],
      'reversible Trash and permanent generated-directory removal must both cross the gate after identity validation',
    ).toHaveLength(2);
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
