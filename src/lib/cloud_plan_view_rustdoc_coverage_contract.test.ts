import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  missingDocsLintLevels,
  publicRustModuleHasOuterDoc
} from './rustModuleInnerAttributes.testSupport';

const cloudPlanViewSource = readFileSync(
  new URL('../../src-tauri/src/cloud_plan_view.rs', import.meta.url),
  'utf8'
);
const crateRootSource = readFileSync(
  new URL('../../src-tauri/src/lib.rs', import.meta.url),
  'utf8'
);

describe('cloud plan view rustdoc ownership', () => {
  it('keeps compiler-enforced documentation on the backend-authored presentation boundary', () => {
    const levels = missingDocsLintLevels(cloudPlanViewSource);
    expect(levels.has('deny')).toBe(true);
    expect(levels.has('allow')).toBe(false);
  });

  it('keeps approval-presentation authority documented at the crate root', () => {
    expect(publicRustModuleHasOuterDoc(crateRootSource, 'cloud_plan_view')).toBe(true);
  });
});
