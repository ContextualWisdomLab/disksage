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

  it('requires allocation-view evidence without over-crediting physical reclaim', () => {
    const measurement = between('fn bounded_dir_size(', 'fn ensure_absolute_project(');
    expect(
      measurement,
      'Unix allocation evidence must use allocated blocks rather than logical length',
    ).toMatch(/\.blocks\(\)/);
    expect(
      measurement,
      'hard-linked entries inside the target view must be deduplicated by filesystem identity',
    ).toMatch(/\.dev\(\)[\s\S]*?\.ino\(\)|\.ino\(\)[\s\S]*?\.dev\(\)/);
    expect(
      measurement,
      'Rust MetadataExt::blocks is defined in 512-byte units; conversion must be explicit and overflow-safe',
    ).toMatch(/(?:checked|saturating)_mul\(512\)/);
    expect(
      source,
      'platforms without allocation evidence must fail closed instead of substituting logical bytes',
    ).toContain('cargo-target-size-allocation-evidence-unsupported');

    const ledger = between('pub fn ledger_reclaim_bytes(', '#[cfg(test)]');
    expect(
      ledger,
      'the current result schema has no proof that external hard links/shared extents released physical storage',
    ).toMatch(/->\s*u64\s*\{\s*(?:let\s+_\s*=\s*[^;]+;\s*)?0\s*\}/);
  });
});
