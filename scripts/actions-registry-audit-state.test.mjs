import test from 'node:test';
import assert from 'node:assert/strict';
import {
  activePullRequestWorkflowPaths,
  auditActionsRegistry,
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

test('malformed workflow identity fails closed instead of becoming dynamic or nullable evidence', () => {
  for (const record of [
    { state: 'active', path: '.github/workflows/unknown.yml' },
    { id: 0, state: 'active', path: '.github/workflows/unknown.yml' },
    { id: -1, state: 'active', path: '.github/workflows/unknown.yml' },
    { id: 1.5, state: 'active', path: '.github/workflows/unknown.yml' },
    { id: '9', state: 'active', path: '.github/workflows/unknown.yml' },
    { id: 9, state: 'active', path: null },
    { id: 9, state: 'active', path: '' },
    { id: 9, state: 'active', path: 42 },
    { id: 9, state: 'active', path: '../escape.yml' },
  ]) {
    assert.throws(
      () => classify(record),
      /actions-workflow-record-invalid/,
      `malformed workflow identity must fail closed: ${JSON.stringify(record)}`,
    );
  }

  assert.deepEqual(
    classify({ id: 10, state: 'active', path: 'dynamic/dependabot' }),
    { classification: 'github-dynamic', path: 'dynamic/dependabot', workflow_id: 10 },
  );
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
      [{ number: 1, head: { sha: sha('b'), repo: { full_name: repo } } }],
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

test('missing protected-main identity fields fail closed', async () => {
  await assert.rejects(
    protectedWorkflowPaths(async (url) => {
      if (url.endsWith('/disksage')) return {};
      throw new Error(`unexpected URL ${url}`);
    }, repo, sha()),
    /default-branch-unavailable/,
  );

  await assert.rejects(
    protectedWorkflowPaths(async (url) => {
      if (url.endsWith('/disksage')) return { default_branch: 'main' };
      if (url.endsWith('/commits/main')) return {};
      throw new Error(`unexpected URL ${url}`);
    }, repo, sha()),
    /protected-main-sha-unavailable/,
  );
});

test('malformed open-PR ownership records fail closed before workflow classification', async () => {
  for (const pullRequests of [
    [null],
    [{}],
    [{ head: null }],
    [{ head: { repo: {} } }],
    [{ head: { repo: { full_name: '' } } }],
    [{ head: { repo: { full_name: repo } } }],
  ]) {
    await assert.rejects(
      activePullRequestWorkflowPaths(async () => {
        throw new Error('tree fetch must not run for malformed ownership evidence');
      }, repo, pullRequests),
      /open-pr-head-invalid/,
      `malformed open-PR ownership evidence must fail closed: ${JSON.stringify(pullRequests)}`,
    );
  }
});

test('deleted-fork open PRs do not block same-repository workflow ownership', async () => {
  const head = sha('6');
  const owned = '.github/workflows/owned-from-live-pr.yml';
  const paths = await activePullRequestWorkflowPaths(async (url) => {
    if (url.includes(`/git/trees/${head}`)) {
      return {
        truncated: false,
        tree: [{ type: 'blob', path: owned }],
      };
    }
    if (url.includes('/pulls/42/files')) {
      return [{ filename: owned, status: 'added' }];
    }
    throw new Error(`unexpected URL ${url}`);
  }, repo, [
    { number: 41, head: { sha: null, repo: null } },
    { number: 42, head: { sha: head, repo: { full_name: repo } } },
  ]);

  assert.deepEqual([...paths], [owned]);
});

test('malformed open-PR number and changed-file evidence fail closed', async () => {
  const head = sha('7');
  await assert.rejects(
    activePullRequestWorkflowPaths(async () => {
      throw new Error('provider must not run before PR number validation');
    }, repo, [{ head: { sha: head, repo: { full_name: repo } } }]),
    /open-pr-number-invalid/,
  );

  for (const invalidFile of [
    null,
    42,
    {},
    { filename: 42, status: 'modified' },
    { filename: '', status: 'modified' },
    { filename: '.github/workflows/test.yml', status: null },
    { filename: '.github/workflows/test.yml', status: '' },
  ]) {
    await assert.rejects(
      activePullRequestWorkflowPaths(async (url) => {
        if (url.includes('/git/trees/')) {
          return {
            truncated: false,
            tree: [{ type: 'blob', path: '.github/workflows/test.yml' }],
          };
        }
        if (url.includes('/pulls/7/files')) return [invalidFile];
        throw new Error(`unexpected URL ${url}`);
      }, repo, [{ number: 7, head: { sha: head, repo: { full_name: repo } } }]),
      /open-pr-file-invalid/,
    );
  }

  await assert.rejects(
    activePullRequestWorkflowPaths(async (url) => {
      if (url.includes('/git/trees/')) return { truncated: false, tree: [] };
      if (url.includes('/pulls/7/files')) return null;
      throw new Error(`unexpected URL ${url}`);
    }, repo, [{ number: 7, head: { sha: head, repo: { full_name: repo } } }]),
    /open-pr-files-invalid/,
  );
});

test('audit fails closed when same-repository open-PR workflow ownership moves mid-audit', async () => {
  const expected = sha('d');
  const newHead = sha('e');
  let pullReads = 0;
  const fetchJson = async (url) => {
    if (url.includes('/actions/workflows')) {
      return {
        total_count: 1,
        workflows: [{ id: 10, state: 'active', path: '.github/workflows/pr-only.yml' }],
      };
    }
    if (url.includes('/pulls?')) {
      pullReads += 1;
      return pullReads <= 2
        ? []
        : [{ number: 7, head: { sha: newHead, repo: { full_name: repo } } }];
    }
    if (url.endsWith('/disksage')) return { default_branch: 'main' };
    if (url.endsWith('/commits/main')) return { sha: expected };
    if (url.includes(`/git/trees/${expected}`)) return { truncated: false, tree: [] };
    if (url.includes(`/git/trees/${newHead}`)) {
      return {
        truncated: false,
        tree: [{ type: 'blob', path: '.github/workflows/pr-only.yml' }],
      };
    }
    throw new Error(`unexpected URL ${url}`);
  };

  await assert.rejects(
    auditActionsRegistry(fetchJson, repo, expected),
    /open-pr-snapshot-moved/,
  );
  assert.equal(pullReads, 4);
});

test('audit fails closed when active PR identity changes at the same head', async () => {
  const expected = sha('4');
  const sharedHead = sha('5');
  let pullReads = 0;
  const fetchJson = async (url) => {
    if (url.includes('/actions/workflows')) {
      return {
        total_count: 1,
        workflows: [{ id: 13, state: 'active', path: '.github/workflows/pr-only.yml' }],
      };
    }
    if (url.includes('/pulls?')) {
      pullReads += 1;
      const number = pullReads <= 2 ? 42 : 43;
      return [{ number, head: { sha: sharedHead, repo: { full_name: repo } } }];
    }
    if (url.includes('/pulls/42/files')) {
      return [{ filename: '.github/workflows/pr-only.yml', status: 'added' }];
    }
    if (url.endsWith('/disksage')) return { default_branch: 'main' };
    if (url.endsWith('/commits/main')) return { sha: expected };
    if (url.includes(`/git/trees/${expected}`)) return { truncated: false, tree: [] };
    if (url.includes(`/git/trees/${sharedHead}`)) {
      return {
        truncated: false,
        tree: [{ type: 'blob', path: '.github/workflows/pr-only.yml' }],
      };
    }
    throw new Error(`unexpected URL ${url}`);
  };

  await assert.rejects(
    auditActionsRegistry(fetchJson, repo, expected),
    /open-pr-snapshot-moved/,
  );
  assert.equal(pullReads, 4);
});

test('audit fails closed when workflow registry identity changes without a count change', async () => {
  const expected = sha('f');
  let workflowReads = 0;
  const fetchJson = async (url) => {
    if (url.includes('/actions/workflows')) {
      workflowReads += 1;
      return {
        total_count: 1,
        workflows: [{
          id: 11,
          state: 'active',
          path: workflowReads === 1
            ? '.github/workflows/orphan-a.yml'
            : '.github/workflows/orphan-b.yml',
        }],
      };
    }
    if (url.includes('/pulls?')) return [];
    if (url.endsWith('/disksage')) return { default_branch: 'main' };
    if (url.endsWith('/commits/main')) return { sha: expected };
    if (url.includes(`/git/trees/${expected}`)) return { truncated: false, tree: [] };
    throw new Error(`unexpected URL ${url}`);
  };

  await assert.rejects(
    auditActionsRegistry(fetchJson, repo, expected),
    /actions-workflow-snapshot-moved/,
  );
  assert.equal(workflowReads, 2);
});

test('audit revalidates protected main after collecting mutable registry evidence', async () => {
  const expected = sha('1');
  const moved = sha('2');
  let mainReads = 0;
  const fetchJson = async (url) => {
    if (url.includes('/actions/workflows')) {
      return {
        total_count: 1,
        workflows: [{ id: 12, state: 'active', path: '.github/workflows/main.yml' }],
      };
    }
    if (url.includes('/pulls?')) return [];
    if (url.endsWith('/disksage')) return { default_branch: 'main' };
    if (url.endsWith('/commits/main')) {
      mainReads += 1;
      return { sha: mainReads === 1 ? expected : moved };
    }
    if (url.includes(`/git/trees/${expected}`)) {
      return {
        truncated: false,
        tree: [{ type: 'blob', path: '.github/workflows/main.yml' }],
      };
    }
    throw new Error(`unexpected URL ${url}`);
  };

  await assert.rejects(
    auditActionsRegistry(fetchJson, repo, expected),
    /protected-main-moved/,
  );
  assert.equal(mainReads, 2);
});

test('active PR ownership excludes stale inherited workflow paths that the PR did not change', async () => {
  const head = sha('9');
  const owned = '.github/workflows/owned-change.yml';
  const inherited = '.github/workflows/stale-inherited.yml';
  const calls = [];
  const paths = await activePullRequestWorkflowPaths(async (url) => {
    calls.push(url);
    if (url.includes(`/git/trees/${head}`)) {
      return {
        truncated: false,
        tree: [
          { type: 'blob', path: inherited },
          { type: 'blob', path: owned },
        ],
      };
    }
    if (url.includes('/pulls/42/files')) {
      return [{ filename: owned, status: 'modified' }];
    }
    throw new Error(`unexpected URL ${url}`);
  }, repo, [{ number: 42, head: { sha: head, repo: { full_name: repo } } }]);

  assert.deepEqual([...paths], [owned]);
  assert.ok(calls.some((url) => url.includes('/pulls/42/files')));
});
