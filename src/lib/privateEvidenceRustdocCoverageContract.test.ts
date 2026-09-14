import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const privateEvidenceSource = readFileSync(
  new URL('../../src-tauri/src/private_evidence.rs', import.meta.url),
  'utf8'
);
const crateRootSource = readFileSync(
  new URL('../../src-tauri/src/lib.rs', import.meta.url),
  'utf8'
);

describe('private evidence rustdoc ownership', () => {
  it('keeps compiler-enforced documentation on the security publication boundary', () => {
    expect(privateEvidenceSource).toContain('#![deny(missing_docs)]');
    expect(privateEvidenceSource).not.toMatch(/allow\s*\(\s*missing_docs\s*\)/);
  });

  it('keeps create-new private evidence authority documented at the crate root', () => {
    expect(crateRootSource).toMatch(
      /\/\/\/[^\n]+\n\s*pub mod private_evidence;/
    );
  });
});
