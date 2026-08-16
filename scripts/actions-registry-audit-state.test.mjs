import test from 'node:test';
import assert from 'node:assert/strict';
import { classifyWorkflowRecord } from './actions-registry-audit.mjs';

const mainPaths = new Set(['.github/workflows/current.yml']);

function classify(record) {
  return classifyWorkflowRecord(record, mainPaths, new Set());
}

test('malformed or unknown workflow state fails closed instead of being called disabled', () => {
  for (const record of [
    null,
    {},
    { id: 6, path: '.github/workflows/unknown.yml' },
    { id: 7, state: null, path: '.github/workflows/unknown.yml' },
    { id: 8, state: 'unexpected', path: '.github/workflows/unknown.yml' },
  ]) {
    assert.throws(
      () => classify(record),
      /actions-workflow-record-invalid/,
      `ambiguous registry record must fail closed: ${JSON.stringify(record)}`,
    );
  }
});

test('documented inactive workflow states remain non-actionable disabled records', () => {
  for (const state of [
    'deleted',
    'disabled_fork',
    'disabled_inactivity',
    'disabled_manually',
  ]) {
    assert.deepEqual(
      classify({ id: 9, state, path: '.github/workflows/old.yml' }),
      { classification: 'disabled', path: '.github/workflows/old.yml', workflow_id: 9 },
    );
  }
});
