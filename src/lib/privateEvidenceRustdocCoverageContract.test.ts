import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  missingDocsLintLevels,
  publicRustModuleHasOuterDoc
} from './rustModuleInnerAttributes.testSupport';

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
    const levels = missingDocsLintLevels(privateEvidenceSource);
    expect(levels.has('deny')).toBe(true);
    expect(levels.has('allow')).toBe(false);
  });

  it('keeps create-new private evidence authority documented at the crate root', () => {
    expect(publicRustModuleHasOuterDoc(crateRootSource, 'private_evidence')).toBe(true);
  });
});
