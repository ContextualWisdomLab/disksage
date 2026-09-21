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
    const openDirectory = sourceSlice(
      'pub(super) fn open_directory',
      'pub(super) fn identity',
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

    // The same handle is reused for owner inspection and rename. GetSecurityInfo
    // requires READ_CONTROL for OWNER_SECURITY_INFORMATION, while the mutation path
    // requires DELETE-capable access. Replacing generic read with DELETE alone would
    // make the ownership gate fail before the handle-bound safety boundary is usable.
    expect(windowsNative).toMatch(/const\s+DELETE(?:_ACCESS)?\s*:\s*u32\s*=\s*0x0*1_?0*0*0\s*;/i);
    expect(windowsNative).toMatch(/const\s+READ_CONTROL\s*:\s*u32\s*=\s*0x0*2_?0*0*0\s*;/i);
    expect(openDirectory).toContain('.access_mode(');
    expect(openDirectory).toMatch(/\.access_mode\([^)]*DELETE(?:_ACCESS)?[^)]*\)/s);
    expect(openDirectory).toMatch(/\.access_mode\([^)]*READ_CONTROL[^)]*\)/s);
    expect(openDirectory).not.toContain('.read(true)');

    // ADR-0002 requires identity to be revalidated immediately before mutation.
    // A handle-bound rename prevents the replacement object from being renamed, but
    // without this last pathname-to-handle check DiskSage could still clean the
    // reviewed object after it has ceased to be the requested target pathname.
    const preMutationRevalidation = windowsDetach.indexOf(
      'windows_native::identity_at(target_dir)',
    );
    const handleDetach = windowsDetach.indexOf(
      'windows_native::rename_opened_directory(&opened.file, &clean_path)',
    );
    expect(preMutationRevalidation).toBeGreaterThanOrEqual(0);
    expect(handleDetach).toBeGreaterThan(preMutationRevalidation);
    const authorizationWindow = windowsDetach.slice(preMutationRevalidation, handleDetach);
    expect(authorizationWindow).toContain('opened.identity');
    expect(authorizationWindow).toContain('cargo-target-dir-replaced');

    expect(windowsDetach).toMatch(
      /windows_native::rename_opened_directory\s*\(\s*&opened\.file\s*,\s*&clean_path\s*\)/,
    );
    expect(windowsDetach).toMatch(
      /windows_native::rename_opened_directory\s*\(\s*&self\.opened\.file\s*,\s*&self\.original_path\s*\)/,
    );
    expect(windowsDetach).not.toContain('std::fs::rename(');
  });
});
