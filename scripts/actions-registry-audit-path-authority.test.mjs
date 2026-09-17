import test from 'node:test';
import assert from 'node:assert/strict';
import {
  classifyWorkflowRecord,
  classifyWorkflowRecords,
} from './actions-registry-audit.mjs';

const emptyPaths = new Set();

/**
 * GitHub's explicit `dynamic/` namespace is the only trusted active identity outside repository
 * workflow paths. Both the exported single-record classifier and the collection audit must fail
 * closed for arbitrary namespaces so no caller can accidentally bypass orphan/drift authority.
 */
test('unexpected active workflow namespaces fail closed at every classifier boundary', () => {
  for (const path of [
    '.GitHub/workflows/case-mismatch.yml',
    '.github/workflows/readme.md',
    'scripts/unexpected-workflow.yml',
    'workflows/unexpected-workflow.yml',
    'dynamicx/dependabot',
    'dynamic/',
  ]) {
    const record = { id: 9001, state: 'active', path };
    assert.throws(
      () => classifyWorkflowRecord(record, emptyPaths, emptyPaths),
      /actions-workflow-path-untrusted/,
      `single-record classifier must reject ${path}`,
    );
    assert.throws(
      () => classifyWorkflowRecords([record], emptyPaths, emptyPaths),
      /actions-workflow-path-untrusted/,
      `collection classifier must reject ${path}`,
    );
  }

  const trustedDynamic = {
    classification: 'github-dynamic',
    path: 'dynamic/dependabot',
    workflow_id: 9002,
  };
  assert.deepEqual(
    classifyWorkflowRecord(
      { id: 9002, state: 'active', path: 'dynamic/dependabot' },
      emptyPaths,
      emptyPaths,
    ),
    trustedDynamic,
  );
  assert.deepEqual(
    classifyWorkflowRecords(
      [{ id: 9002, state: 'active', path: 'dynamic/dependabot' }],
      emptyPaths,
      emptyPaths,
    ),
    [trustedDynamic],
  );
});

test('parent traversal aliases never normalize into workflow authority', () => {
  for (const path of [
    'dynamic/../.github/workflows/forged.yml',
    '.github/workflows/nested/../forged.yml',
  ]) {
    const record = { id: 9003, state: 'active', path };
    assert.throws(
      () => classifyWorkflowRecord(record, emptyPaths, emptyPaths),
      /actions-workflow-record-invalid/,
      `single-record classifier must reject traversal alias ${path}`,
    );
    assert.throws(
      () => classifyWorkflowRecords([record], emptyPaths, emptyPaths),
      /actions-workflow-record-invalid/,
      `collection classifier must reject traversal alias ${path}`,
    );
  }
});
