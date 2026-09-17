import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

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
    expect(cloudPlanViewSource).toContain('#![deny(missing_docs)]');
    expect(cloudPlanViewSource).not.toMatch(/allow\s*\(\s*missing_docs\s*\)/);
  });

  it('keeps approval-presentation authority documented at the crate root', () => {
    expect(crateRootSource).toMatch(
      /\/\/\/[^\n]+\n\s*pub mod cloud_plan_view;/
    );
  });
});
