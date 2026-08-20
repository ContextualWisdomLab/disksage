import test from 'node:test';
import assert from 'node:assert/strict';
import { classifyWorkflowRecord } from './actions-registry-audit.mjs';

const emptyPaths = new Set();

/**
 * GitHub-owned dynamic workflow identities use the explicit `dynamic/` namespace. An arbitrary
 * active registry path must never inherit that trust merely because it is outside
 * `.github/workflows/`; otherwise case mistakes or unexpected namespaces can evade orphan/drift
 * detection.
 */
test('unexpected active workflow namespaces fail closed instead of becoming github-dynamic', () => {
  for (const path of [
    '.GitHub/workflows/case-mismatch.yml',
    'scripts/unexpected-workflow.yml',
    'workflows/unexpected-workflow.yml',
    'dynamicx/dependabot',
  ]) {
    assert.throws(
      () => classifyWorkflowRecord(
        { id: 9001, state: 'active', path },
        emptyPaths,
        emptyPaths,
      ),
      /actions-workflow-path-untrusted/,
      path,
    );
  }

  assert.deepEqual(
    classifyWorkflowRecord(
      { id: 9002, state: 'active', path: 'dynamic/dependabot' },
      emptyPaths,
      emptyPaths,
    ),
    {
      classification: 'github-dynamic',
      path: 'dynamic/dependabot',
      workflow_id: 9002,
    },
  );
});
