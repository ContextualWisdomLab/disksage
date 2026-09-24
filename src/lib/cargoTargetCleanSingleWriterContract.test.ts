import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const script = readFileSync('scripts/guarded-cargo-target-clean.sh', 'utf8');

describe('cargo target clean single-writer contract', () => {
  it('keeps the shell entry point as a thin launcher for the Rust deletion authority', () => {
    expect(script).toContain('disksage-cargo-target-clean');
    expect(script).toMatch(/\bexec\b/);

    // Safety and mutation decisions belong to cargo_target_reclaim. Keeping copies
    // here creates a second deletion authority that can drift behind the Rust path.
    expect(script).not.toContain('LSOF_BIN');
    expect(script).not.toContain('OWNER_UID');
    expect(script).not.toContain('python3 - "$PROJECT_REAL"');
    expect(script).not.toMatch(/"\$CARGO"\s+clean\b/);
  });
});
