import test from 'node:test';
import assert from 'node:assert/strict';
import {
  auditActionsRegistry,
  classifyWorkflowRecord,
  classifyWorkflowRecords,
  listAllWorkflowRecords,
  normalizeWorkflowPath,
  protectedWorkflowPaths,
} from './actions-registry-audit.mjs';

const repo = 'ContextualWisdomLab/disksage';
const sha = (char = 'a') => char.repeat(40);

function protectedFetch(expected, tree) {
  return async (url) => {
    if (url.endsWith('/disksage')) return { default_branch: 'main' };
    if (url.endsWith('/commits/main')) return { sha: expected };
    if (url.includes('/git/trees/')) return tree;
    throw new Error(`unexpected URL ${url}`);
  };
}

test('normalizes paths fail closed without case folding', () => {
  assert.equal(normalizeWorkflowPath('./.github//workflows/test.yml'), '.github/workflows/test.yml');
  assert.equal(normalizeWorkflowPath('.github\\workflows\\test.yaml'), '.github/workflows/test.yaml');
  assert.equal(normalizeWorkflowPath('././.github/workflows/a.yml'), '.github/workflows/a.yml');
  for (const value of [null, 42, '', '.', '../escape.yml', '/absolute.yml']) {
    assert.equal(normalizeWorkflowPath(value), null);
  }
});

test('classifies registry authority by exact protected path', () => {
  const paths = new Set(['.github/workflows/repair-current.yml']);
  const cases = [
    [{ id: 1, state: 'active', path: '.github/workflows/repair-current.yml' }, 'present', 1],
    [{ state: 'active', path: '.github/workflows/repair-current.yml' }, 'present', null],
    [{ id: 2, state: 'active', path: '.github/workflows/old.yml' }, 'orphaned-deleted', 2],
    [{ state: 'active', path: '.github/workflows/orphan.yml' }, 'orphaned-deleted', null],
    [{ id: 3, state: 'disabled_manually', path: '.github/workflows/old.yml' }, 'disabled', 3],
    [null, 'disabled', null],
    [{ id: 4, state: 'active', path: 'dynamic/dependabot' }, 'github-dynamic', 4],
    [{ state: 'active', path: null }, 'github-dynamic', null],
    [{ id: 5, state: 'active', path: '.GitHub/workflows/repair-current.yml' }, 'github-dynamic', 5],
  ];
  for (const [record, classification, workflowId] of cases) {
    const result = classifyWorkflowRecord(record, paths);
    assert.equal(result.classification, classification);
    assert.equal(result.workflow_id, workflowId);
  }
  assert.deepEqual(classifyWorkflowRecords([], paths), []);
});

test('paginates all registry pages and rejects malformed responses', async () => {
  const calls = [];
  const first = Array.from({ length: 100 }, (_, index) => ({ id: index + 1 }));
  const records = await listAllWorkflowRecords(async (url) => {
    calls.push(url);
    return url.endsWith('page=1') ? { workflows: first } : { workflows: [{ id: 101 }] };
  }, repo);
  assert.equal(records.length, 101);
  assert.equal(calls.length, 2);
  await assert.rejects(listAllWorkflowRecords(async () => null, repo), /actions-workflow-list-invalid/);
  await assert.rejects(listAllWorkflowRecords(async () => ({ workflows: null }), repo), /actions-workflow-list-invalid/);
});

test('binds workflow discovery to exact complete protected main', async () => {
  const tree = {
    truncated: false,
    tree: [
      null,
      { type: 'tree', path: '.github/workflows' },
      { type: 'blob', path: null },
      { type: 'blob', path: 'src/not-workflow.yml' },
      { type: 'blob', path: '.github/workflows/readme.md' },
      { type: 'blob', path: '.github/workflows/a.yaml' },
      { type: 'blob', path: '.github/workflows/b.yml' },
    ],
  };
  assert.deepEqual(
    [...await protectedWorkflowPaths(protectedFetch(sha(), tree), repo, sha())].sort(),
    ['.github/workflows/a.yaml', '.github/workflows/b.yml'],
  );
});

test('fails closed for default-branch, revision and tree ambiguity', async () => {
  await assert.rejects(protectedWorkflowPaths(async () => ({ default_branch: '' }), repo, sha()), /default-branch-unavailable/);
  await assert.rejects(protectedWorkflowPaths(protectedFetch(sha('b'), { truncated: false, tree: [] }), repo, sha()), /protected-main-moved/);
  await assert.rejects(protectedWorkflowPaths(protectedFetch(sha(), null), repo, sha()), /protected-main-tree-incomplete/);
  await assert.rejects(protectedWorkflowPaths(protectedFetch(sha(), { truncated: true, tree: [] }), repo, sha()), /protected-main-tree-incomplete/);
  await assert.rejects(protectedWorkflowPaths(protectedFetch(sha(), { truncated: false, tree: null }), repo, sha()), /protected-main-tree-incomplete/);
});

test('propagates API permission/provider failures', async () => {
  await assert.rejects(listAllWorkflowRecords(async () => { throw new Error('forbidden'); }, repo), /forbidden/);
});

test('builds a complete audit with exact orphan identities', async () => {
  const expected = sha('c');
  const fetchJson = async (url) => {
    if (url.includes('/actions/workflows')) return { workflows: [
      { id: 7, state: 'active', path: '.github/workflows/test.yml' },
      { id: 8, state: 'active', path: '.github/workflows/old-repair.yml' },
    ] };
    if (url.endsWith('/disksage')) return { default_branch: 'main' };
    if (url.endsWith('/commits/main')) return { sha: expected };
    if (url.includes('/git/trees/')) return { truncated: false, tree: [{ type: 'blob', path: '.github/workflows/test.yml' }] };
    throw new Error(`unexpected URL ${url}`);
  };
  const report = await auditActionsRegistry(fetchJson, repo, expected);
  assert.equal(report.schema_version, 1);
  assert.equal(report.repository, repo);
  assert.equal(report.protected_main_sha, expected);
  assert.equal(report.total_records, 2);
  assert.deepEqual(report.orphaned_records.map((entry) => entry.workflow_id), [8]);
  assert.equal(report.classifications.length, 2);
});
