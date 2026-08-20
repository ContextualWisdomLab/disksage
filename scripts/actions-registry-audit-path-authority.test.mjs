import test from 'node:test';
import assert from 'node:assert/strict';
import { classifyWorkflowRecords } from './actions-registry-audit.mjs';

const emptyPaths = new Set();

/**
 * The authoritative collection audit must accept only GitHub's explicit `dynamic/` namespace for
 * active non-repository workflow identities. Arbitrary active paths must fail closed so case
 * mistakes or unexpected namespaces cannot evade orphan/drift detection.
 */
test('unexpected active workflow namespaces fail closed instead of becoming trusted dynamic records', () => {
  for (const path of [
    '.GitHub/workflows/case-mismatch.yml',
    'scripts/unexpected-workflow.yml',
    'workflows/unexpected-workflow.yml',
    'dynamicx/dependabot',
  ]) {
    assert.throws(
      () => classifyWorkflowRecords(
        [{ id: 9001, state: 'active', path }],
        emptyPaths,
        emptyPaths,
      ),
      /actions-workflow-path-untrusted/,
      path,
    );
  }

  assert.deepEqual(
    classifyWorkflowRecords(
      [{ id: 9002, state: 'active', path: 'dynamic/dependabot' }],
      emptyPaths,
      emptyPaths,
    ),
    [{
      classification: 'github-dynamic',
      path: 'dynamic/dependabot',
      workflow_id: 9002,
    }],
  );
});
