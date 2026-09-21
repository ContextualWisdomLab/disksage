import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const source = readFileSync('src-tauri/src/cargo_target_reclaim.rs', 'utf8');

function sourceSlice(startMarker: string, endMarker: string): string {
  const start = source.indexOf(startMarker);
  expect(start).toBeGreaterThanOrEqual(0);
  const end = source.indexOf(endMarker, start + startMarker.length);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe('Windows Cargo target detach object-authority contract', () => {
  it('renames the reviewed open directory object instead of reselecting its pathname', () => {
    const windowsNative = sourceSlice(
      '#[cfg(windows)]\nmod windows_native',
      '/// Active-use / ownership gate invoked before deletion.',
    );
    const windowsDetach = sourceSlice(
      '#[cfg(windows)]\nfn detach_verified_target_dir',
      '#[cfg(not(any(unix, windows)))]\nstruct DetachedTargetDir',
    );

    // Microsoft documents FileRenameInfo as a SetFileInformationByHandle operation.
    // The source handle is therefore the mutation authority; re-resolving target_dir
    // would allow a same-path replacement to become a transient mutation subject.
    expect(windowsNative).toContain('SetFileInformationByHandle');
    expect(windowsNative).toMatch(/fn\s+rename_opened_directory\s*\(\s*file:\s*&File,/);
    expect(windowsDetach).toMatch(
      /windows_native::rename_opened_directory\s*\(\s*&opened\.file\s*,\s*&clean_path\s*\)/,
    );
    expect(windowsDetach).not.toMatch(/std::fs::rename\s*\(\s*target_dir\s*,/);
  });
});
