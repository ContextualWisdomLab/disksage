import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  missingDocsLintLevels,
  publicRustModuleHasOuterDoc
} from './rustModuleInnerAttributes.testSupport';

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
    const levels = missingDocsLintLevels(naruonCapacitySource);
    expect(levels.has('deny')).toBe(true);
    expect(levels.has('allow')).toBe(false);
  });

  it('documents the redacted capacity handoff at the crate root', () => {
    expect(publicRustModuleHasOuterDoc(crateRootSource, 'naruon_capacity')).toBe(true);
  });
});
