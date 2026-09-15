import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');
const devArtifactsSource = readFileSync('src-tauri/src/dev_artifacts.rs', 'utf8');

describe('development-artifact post-mutation recovery boundary', () => {
  it('exposes only crate-internal typed retry entry points from deletion safety', () => {
    expect(safetySource).toMatch(
      /pub\(crate\)\s+fn\s+retry_pending_trash_cleanup\s*\([\s\S]{0,900}->\s*Option\s*<\s*Result\s*<\s*\(\)\s*,\s*SafetyError\s*>\s*>/,
    );
    expect(safetySource).toMatch(
      /pub\(crate\)\s+fn\s+retry_pending_permanent_cleanup\s*\([\s\S]{0,900}->\s*Option\s*<\s*Result\s*<\s*\(\)\s*,\s*SafetyError\s*>\s*>/,
    );
    expect(
      safetySource,
      'recovery-only authority must stay crate-internal rather than widening destructive APIs',
    ).not.toMatch(/pub\s+fn\s+retry_pending_(trash|permanent)_cleanup/);
  });

  it('branches on typed recovery presence/result rather than presentation text', () => {
    expect(devArtifactsSource).toContain('retry_pending_trash_cleanup');
    expect(devArtifactsSource).toContain('retry_pending_permanent_cleanup');
    expect(
      devArtifactsSource,
      'human error wording must not be a deletion-safety control-flow protocol',
    ).not.toContain('starts_with("mutation completed;")');
  });
});
