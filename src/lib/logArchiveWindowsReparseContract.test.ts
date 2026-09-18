import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const logArchive = readFileSync('src-tauri/src/log_archive.rs', 'utf8');
const scanner = readFileSync('src-tauri/src/scanner.rs', 'utf8');

describe('log archive Windows reparse traversal contract', () => {
  it('reuses the canonical scanner entry filter before evaluating archive candidates', () => {
    expect(scanner).toMatch(
      /pub\(crate\) fn keep_entry[\s\S]*FILE_ATTRIBUTE_REPARSE_POINT/,
    );
    expect(logArchive).toMatch(
      /WalkDir::new\(&options\.root\)[\s\S]*?\.filter_entry\(crate::scanner::keep_entry\)/,
    );
  });

  it('retains a real Windows junction acceptance fixture in the Rust owner', () => {
    expect(logArchive).toMatch(
      /#\[cfg\(windows\)\][\s\S]*fn windows_junction_is_not_traversed_for_archive_mutation\s*\(/,
    );
  });
});
