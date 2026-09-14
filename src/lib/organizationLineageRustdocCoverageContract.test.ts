import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { missingDocsLintLevels } from './rustModuleInnerAttributes.testSupport';

const organizationLineageSource = readFileSync(
  new URL('../../src-tauri/src/organization_lineage.rs', import.meta.url),
  'utf8'
);
const crateRootSource = readFileSync(
  new URL('../../src-tauri/src/lib.rs', import.meta.url),
  'utf8'
);

describe('organization lineage rustdoc ownership', () => {
  it('keeps compiler-enforced documentation on the public lineage handoff', () => {
    const levels = missingDocsLintLevels(organizationLineageSource);
    expect(levels.has('deny')).toBe(true);
    expect(levels.has('allow')).toBe(false);
  });

  it('keeps the path-free ontology handoff documented at the crate root', () => {
    expect(crateRootSource).toMatch(
      /\/\/\/[^\n]+\n\s*pub mod organization_lineage;/
    );
  });
});
