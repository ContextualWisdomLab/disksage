import test from 'node:test';
import assert from 'node:assert/strict';
import {
  activePullRequestWorkflowPaths,
  classifyWorkflowRecord,
  protectedWorkflowPaths,
} from './actions-registry-audit.mjs';

const repo = 'ContextualWisdomLab/disksage';
const mainPaths = new Set(['.github/workflows/current.yml']);
const sha = (char = 'a') => char.repeat(40);

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

test('workflow trees require an explicit non-truncated provider assertion', async () => {
  await assert.rejects(
    activePullRequestWorkflowPaths(
      async () => ({ tree: [{ type: 'blob', path: '.github/workflows/pr.yml' }] }),
      repo,
      [{ head: { sha: sha('b'), repo: { full_name: repo } } }],
    ),
    /workflow-tree-incomplete/,
  );

  const expected = sha('c');
  const fetchJson = async (url) => {
    if (url.endsWith('/disksage')) return { default_branch: 'main' };
    if (url.endsWith('/commits/main')) return { sha: expected };
    if (url.includes('/git/trees/')) {
      return { tree: [{ type: 'blob', path: '.github/workflows/main.yml' }] };
    }
    throw new Error(`unexpected URL ${url}`);
  };
  await assert.rejects(
    protectedWorkflowPaths(fetchJson, repo, expected),
    /protected-main-tree-incomplete/,
  );
});
