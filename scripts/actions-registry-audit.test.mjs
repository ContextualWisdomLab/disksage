import test from 'node:test';
import assert from 'node:assert/strict';
import {
  activePullRequestWorkflowPaths,
  auditActionsRegistry,
  classifyWorkflowRecord,
  classifyWorkflowRecords,
  listAllOpenPullRequests,
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
  for (const value of [null, 42, '', '.', '../escape.yml', '/absolute.yml']) assert.equal(normalizeWorkflowPath(value), null);
});

test('classifies main, active-PR, orphaned, disabled and dynamic workflow authority', () => {
  const mainPaths = new Set(['.github/workflows/repair-current.yml']);
  const activePrPaths = new Set(['.github/workflows/repair-pr186.yml']);
  const cases = [
    [{ id: 1, state: 'active', path: '.github/workflows/repair-current.yml' }, 'present', 1, undefined],
    [{ state: 'active', path: '.github/workflows/repair-current.yml' }, 'present', null, undefined],
    [{ id: 2, state: 'active', path: '.github/workflows/repair-pr186.yml' }, 'unresolved', 2, 'active-pr-workflow'],
    [{ state: 'active', path: '.github/workflows/repair-pr186.yml' }, 'unresolved', null, 'active-pr-workflow'],
    [{ state: 'active', path: '.github/workflows/orphan.yml' }, 'orphaned-deleted', null, undefined],
    [{ id: 3, state: 'disabled_manually', path: '.github/workflows/old.yml' }, 'disabled', 3, undefined],
    [null, 'disabled', null, undefined],
    [{ id: 4, state: 'active', path: 'dynamic/dependabot' }, 'github-dynamic', 4, undefined],
    [{ state: 'active', path: null }, 'github-dynamic', null, undefined],
    [{ id: 5, state: 'active', path: '.GitHub/workflows/repair-current.yml' }, 'github-dynamic', 5, undefined],
  ];
  for (const [record, classification, workflowId, reason] of cases) {
    const result = classifyWorkflowRecord(record, mainPaths, activePrPaths);
    assert.equal(result.classification, classification);
    assert.equal(result.workflow_id, workflowId);
    assert.equal(result.reason, reason);
  }
  assert.deepEqual(classifyWorkflowRecords([], mainPaths), []);
});

test('paginates complete workflow and open-PR collections', async () => {
  const workflows = Array.from({ length: 100 }, (_, index) => ({ id: index + 1 }));
  const workflowCalls = [];
  const records = await listAllWorkflowRecords(async (url) => {
    workflowCalls.push(url);
    return url.endsWith('page=1')
      ? { total_count: 101, workflows }
      : { total_count: 101, workflows: [{ id: 101 }] };
  }, repo);
  assert.equal(records.length, 101);
  assert.equal(workflowCalls.length, 2);

  const pulls = Array.from({ length: 100 }, (_, index) => ({ number: index + 1 }));
  const pullCalls = [];
  const openPulls = await listAllOpenPullRequests(async (url) => {
    pullCalls.push(url);
    return url.endsWith('page=1') ? pulls : [{ number: 101 }];
  }, repo);
  assert.equal(openPulls.length, 101);
  assert.equal(pullCalls.length, 2);
});

test('fails closed when workflow registry pagination is shorter than total_count', async () => {
  const workflowCalls = [];
  await assert.rejects(
    listAllWorkflowRecords(async (url) => {
      workflowCalls.push(url);
      return { total_count: 101, workflows: [{ id: 1 }] };
    }, repo),
    /actions-workflow-list-incomplete/,
  );
  assert.equal(workflowCalls.length, 1);
});

test('fails closed when workflow registry total_count moves across pages', async () => {
  const workflows = Array.from({ length: 100 }, (_, index) => ({ id: index + 1 }));
  await assert.rejects(
    listAllWorkflowRecords(async (url) => url.endsWith('page=1')
      ? { total_count: 101, workflows }
      : { total_count: 102, workflows: [{ id: 101 }, { id: 102 }] }, repo),
    /actions-workflow-list-moved/,
  );
});

test('fails closed when workflow pages overshoot the announced total', async () => {
  const workflows = Array.from({ length: 100 }, (_, index) => ({ id: index + 1 }));
  await assert.rejects(
    listAllWorkflowRecords(async (url) => url.endsWith('page=1')
      ? { total_count: 101, workflows }
      : { total_count: 101, workflows: [{ id: 101 }, { id: 102 }] }, repo),
    /actions-workflow-list-incomplete/,
  );
});

test('rejects malformed registry and open-PR list evidence', async () => {
  await assert.rejects(listAllWorkflowRecords(async () => null, repo), /actions-workflow-list-invalid/);
  await assert.rejects(listAllWorkflowRecords(async () => ({ total_count: 0, workflows: null }), repo), /actions-workflow-list-invalid/);
  await assert.rejects(listAllWorkflowRecords(async () => ({ workflows: [] }), repo), /actions-workflow-list-invalid/);
  await assert.rejects(listAllWorkflowRecords(async () => ({ total_count: -1, workflows: [] }), repo), /actions-workflow-list-invalid/);
  await assert.rejects(listAllWorkflowRecords(async () => ({ total_count: 0.5, workflows: [] }), repo), /actions-workflow-list-invalid/);
  assert.deepEqual(await listAllWorkflowRecords(async () => ({ total_count: 0, workflows: [] }), repo), []);
  await assert.rejects(listAllOpenPullRequests(async () => null, repo), /open-pr-list-invalid/);
});

test('resolves only same-repository active-PR workflow paths from exact immutable heads', async () => {
  const pulls = [
    { head: { sha: sha('b'), repo: { full_name: repo } } },
    { head: { sha: sha('b'), repo: { full_name: repo } } },
    { head: { sha: sha('c'), repo: { full_name: 'fork/disksage' } } },
  ];
  const calls = [];
  const paths = await activePullRequestWorkflowPaths(async (url) => {
    calls.push(url);
    return {
      truncated: false,
      tree: [
        { type: 'blob', path: '.github/workflows/repair-current.yml' },
        { type: 'blob', path: '.github/workflows/readme.md' },
        { type: 'tree', path: '.github/workflows' },
      ],
    };
  }, repo, pulls);
  assert.deepEqual([...paths], ['.github/workflows/repair-current.yml']);
  assert.equal(calls.length, 1);
});

test('fails closed for malformed active-PR head or tree evidence', async () => {
  await assert.rejects(
    activePullRequestWorkflowPaths(async () => ({ truncated: false, tree: [] }), repo, [
      { head: { sha: 'bad', repo: { full_name: repo } } },
    ]),
    /open-pr-head-invalid/,
  );
  await assert.rejects(
    activePullRequestWorkflowPaths(async () => null, repo, [
      { head: { sha: sha(), repo: { full_name: repo } } },
    ]),
    /workflow-tree-incomplete/,
  );
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
  assert.deepEqual([...await protectedWorkflowPaths(protectedFetch(sha(), tree), repo, sha())].sort(), [
    '.github/workflows/a.yaml', '.github/workflows/b.yml',
  ]);
});

test('fails closed for default-branch, revision and protected-tree ambiguity', async () => {
  await assert.rejects(protectedWorkflowPaths(async () => ({ default_branch: '' }), repo, sha()), /default-branch-unavailable/);
  await assert.rejects(protectedWorkflowPaths(protectedFetch(sha('b'), { truncated: false, tree: [] }), repo, sha()), /protected-main-moved/);
  await assert.rejects(protectedWorkflowPaths(protectedFetch(sha(), null), repo, sha()), /protected-main-tree-incomplete/);
  await assert.rejects(protectedWorkflowPaths(protectedFetch(sha(), { truncated: true, tree: [] }), repo, sha()), /protected-main-tree-incomplete/);
  await assert.rejects(protectedWorkflowPaths(protectedFetch(sha(), { truncated: false, tree: null }), repo, sha()), /protected-main-tree-incomplete/);
});

test('propagates provider and permission failures', async () => {
  await assert.rejects(listAllWorkflowRecords(async () => { throw new Error('forbidden'); }, repo), /forbidden/);
});

test('builds a complete audit without disabling active-PR workflow owners', async () => {
  const expected = sha('d');
  const prHead = sha('e');
  const fetchJson = async (url) => {
    if (url.includes('/actions/workflows')) return { total_count: 3, workflows: [
      { id: 7, state: 'active', path: '.github/workflows/test.yml' },
      { id: 8, state: 'active', path: '.github/workflows/old-repair.yml' },
      { id: 9, state: 'active', path: '.github/workflows/repair-pr186.yml' },
    ] };
    if (url.includes('/pulls?')) return [{ head: { sha: prHead, repo: { full_name: repo } } }];
    if (url.endsWith('/disksage')) return { default_branch: 'main' };
    if (url.endsWith('/commits/main')) return { sha: expected };
    if (url.includes(`/git/trees/${expected}`)) return { truncated: false, tree: [{ type: 'blob', path: '.github/workflows/test.yml' }] };
    if (url.includes(`/git/trees/${prHead}`)) return { truncated: false, tree: [{ type: 'blob', path: '.github/workflows/repair-pr186.yml' }] };
    throw new Error(`unexpected URL ${url}`);
  };
  const report = await auditActionsRegistry(fetchJson, repo, expected);
  assert.equal(report.schema_version, 1);
  assert.equal(report.total_records, 3);
  assert.deepEqual(report.orphaned_records.map((entry) => entry.workflow_id), [8]);
  assert.deepEqual(report.unresolved_records.map((entry) => entry.workflow_id), [9]);
  assert.equal(report.classifications.length, 3);
});
