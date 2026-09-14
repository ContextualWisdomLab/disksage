import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const contentDigestSource = readFileSync(
  new URL('../../src-tauri/src/content_digest.rs', import.meta.url),
  'utf8'
);
const crateRootSource = readFileSync(
  new URL('../../src-tauri/src/lib.rs', import.meta.url),
  'utf8'
);

describe('content digest rustdoc ownership', () => {
  it('keeps compiler-enforced 100% documentation on the module public surface', () => {
    expect(contentDigestSource).toContain('#![deny(missing_docs)]');
    expect(contentDigestSource).not.toMatch(/allow\s*\(\s*missing_docs\s*\)/);
  });

  it('documents the public module boundary at the crate root', () => {
    expect(crateRootSource).toMatch(
      /\/\/\/[^\n]+\n\s*pub mod content_digest;/
    );
  });
});
