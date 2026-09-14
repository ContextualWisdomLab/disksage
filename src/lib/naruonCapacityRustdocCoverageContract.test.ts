import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const naruonCapacitySource = readFileSync(
  new URL('../../src-tauri/src/naruon_capacity.rs', import.meta.url),
  'utf8'
);
const crateRootSource = readFileSync(
  new URL('../../src-tauri/src/lib.rs', import.meta.url),
  'utf8'
);

describe('Naruon capacity rustdoc ownership', () => {
  it('keeps compiler-enforced documentation on the public capacity envelope', () => {
    expect(naruonCapacitySource).toContain('#![deny(missing_docs)]');
    expect(naruonCapacitySource).not.toMatch(/allow\s*\(\s*missing_docs\s*\)/);
  });

  it('documents the redacted capacity handoff at the crate root', () => {
    expect(crateRootSource).toMatch(
      /\/\/\/[^\n]+\n\s*pub mod naruon_capacity;/
    );
  });
});
