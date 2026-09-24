import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const source = readFileSync('src-tauri/src/unix_capability_cleanup.rs', 'utf8');

describe('Unix capability cleanup mount boundary', () => {
  it('requires Linux descriptor-relative opens to reject mount transitions, including bind mounts', () => {
    expect(
      source,
      'st_dev alone cannot distinguish a same-filesystem bind mount; Linux descent must ask the kernel not to cross mount points',
    ).toContain('RESOLVE_NO_XDEV');
    expect(
      source,
      'the Linux mount boundary must be enforced during descriptor-relative open, not by a pathname pre-check',
    ).toMatch(/openat2|SYS_openat2/);
  });

  it('keeps a device check as a second fail-closed boundary for non-Linux Unix adapters', () => {
    expect(source).toContain('cargo-target-capability-cross-device');
    expect(source).toMatch(/st_dev/);
  });
});
