import {
  linkSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const source = readFileSync('src-tauri/src/cargo_target_reclaim.rs', 'utf8');

function between(start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing source marker: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe('cargo target reclaim accounting contract', () => {
  it('real hardlink evidence shows logical length is not reclaimed allocation', () => {
    const root = mkdtempSync(join(tmpdir(), 'disksage-cargo-hardlink-'));
    try {
      const targetEntry = join(root, 'target-entry.bin');
      const retainedEntry = join(root, 'retained-entry.bin');
      const payload = Buffer.alloc(128 * 1024, 0x5a);
      writeFileSync(targetEntry, payload);
      linkSync(targetEntry, retainedEntry);

      const linked = statSync(targetEntry);
      expect(linked.nlink).toBeGreaterThanOrEqual(2);
      expect(linked.size).toBe(payload.length);

      unlinkSync(targetEntry);

      const retained = statSync(retainedEntry);
      expect(retained.nlink).toBeGreaterThanOrEqual(1);
      expect(retained.size).toBe(payload.length);
      expect(readFileSync(retainedEntry)).toEqual(payload);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('does not turn logical file length reduction into buyer-visible reclaim credit', () => {
    const measurement = between('fn bounded_dir_size(', 'fn ensure_absolute_project(');
    expect(
      measurement,
      'logical file length can disappear from target/ while the same allocated object remains through a hard link',
    ).not.toMatch(/\.metadata\(\)[\s\S]*?\.len\(\)/);

    const ledger = between('pub fn ledger_reclaim_bytes(', '#[cfg(test)]');
    expect(
      ledger,
      'ledger reclaim credit requires allocation-backed evidence; raw target-tree logical reduction is not enough',
    ).not.toMatch(/result\.observed_reduction_bytes/);
  });
});
