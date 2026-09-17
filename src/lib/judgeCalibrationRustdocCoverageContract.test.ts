import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  missingDocsLintLevels,
  publicRustModuleHasOuterDoc
} from './rustModuleInnerAttributes.testSupport';

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
    const levels = missingDocsLintLevels(judgeCalibrationSource);
    expect(levels.has('deny')).toBe(true);
    expect(levels.has('allow')).toBe(false);
  });

  it('documents the calibration authority at the crate root', () => {
    expect(publicRustModuleHasOuterDoc(crateRootSource, 'judge_calibration')).toBe(true);
  });
});
