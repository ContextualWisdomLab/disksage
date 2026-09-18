import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const source = readFileSync('src-tauri/src/homebrew_audit.rs', 'utf8');

describe('Homebrew audit Windows compile boundary', () => {
  it('keeps the mount-based atime probe Unix-only and provides a fail-closed non-Unix path', () => {
    expect(
      source,
      'the mount-based atime probe uses the Unix-only Command import and must be cfg-gated with the rest of the Unix process adapter',
    ).toMatch(/#\[cfg\(unix\)\]\s*fn volume_atime_unreliable\(prefix: &Path\) -> bool/);

    expect(
      source,
      'non-Unix builds need an explicit fail-closed volume-atime implementation instead of compiling the Unix mount command',
    ).toMatch(/#\[cfg\(not\(unix\)\)\]\s*fn volume_atime_unreliable\([^)]*\) -> bool\s*\{\s*true\s*\}/s);
  });

  it('keeps Homebrew command execution unsupported on non-Unix platforms', () => {
    expect(source).toContain('Err("homebrew-audit-unsupported-platform".into())');
  });
});
