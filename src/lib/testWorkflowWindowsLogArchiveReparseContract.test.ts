import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const workflow = readFileSync('.github/workflows/test.yml', 'utf8');

describe('shared Test owner Windows log-archive reparse admission', () => {
  it('executes the real junction regression when the bounded-context contract is present', () => {
    expect(workflow).toContain("Test-Path 'src/lib/logArchiveWindowsReparseContract.test.ts'");
    expect(workflow).toContain(
      'log_archive::tests::windows_junction_is_not_traversed_for_archive_mutation',
    );
    expect(workflow).toMatch(
      /cargo test --manifest-path src-tauri\/Cargo\.toml --locked --lib log_archive::tests::windows_junction_is_not_traversed_for_archive_mutation -- --exact/,
    );
  });
});
