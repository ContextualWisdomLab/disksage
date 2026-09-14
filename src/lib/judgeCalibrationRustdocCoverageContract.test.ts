import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const judgeCalibrationSource = readFileSync(
  new URL('../../src-tauri/src/judge_calibration.rs', import.meta.url),
  'utf8'
);
const crateRootSource = readFileSync(
  new URL('../../src-tauri/src/lib.rs', import.meta.url),
  'utf8'
);

describe('judge calibration rustdoc ownership', () => {
  it('keeps compiler-enforced documentation on the public calibration surface', () => {
    expect(judgeCalibrationSource).toContain('#![deny(missing_docs)]');
    expect(judgeCalibrationSource).not.toMatch(/allow\s*\(\s*missing_docs\s*\)/);
  });

  it('documents the calibration authority at the crate root', () => {
    expect(crateRootSource).toMatch(
      /\/\/\/[^\n]+\n\s*pub mod judge_calibration;/
    );
  });
});
