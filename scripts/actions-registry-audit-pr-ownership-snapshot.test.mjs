import test from 'node:test';
import assert from 'node:assert/strict';
import {
  activePullRequestWorkflowPaths,
  auditActionsRegistry,
} from './actions-registry-audit.mjs';

const repository = 'ContextualWisdomLab/disksage';
const mainSha = 'a'.repeat(40);
const headSha = 'b'.repeat(40);
const workflowPath = '.github/workflows/pr-only.yml';

test('fails closed when same-head PR workflow ownership changes during the audit', async () => {
  let pullReads = 0;
  let changedFileReads = 0;

  await assert.rejects(
    auditActionsRegistry(async (url) => {
      if (url.includes('/actions/workflows')) {
        return {
          total_count: 1,
          workflows: [{ id: 41, state: 'active', path: workflowPath }],
        };
      }
      if (url.includes('/pulls?')) {
        pullReads += 1;
        return [{
          number: 42,
          head: { sha: headSha, repo: { full_name: repository } },
        }];
      }
      if (url.includes('/pulls/42/files')) {
        changedFileReads += 1;
        return changedFileReads === 1
          ? [{ filename: workflowPath, status: 'added' }]
          : [];
      }
      if (url.endsWith('/disksage')) return { default_branch: 'main' };
      if (url.endsWith('/commits/main')) return { sha: mainSha };
      if (url.includes(`/git/trees/${mainSha}`)) {
        return { truncated: false, tree: [] };
      }
      if (url.includes(`/git/trees/${headSha}`)) {
        return {
          truncated: false,
          tree: [{ type: 'blob', path: workflowPath }],
        };
      }
      throw new Error(`unexpected URL ${url}`);
    }, repository, mainSha),
    /open-pr-workflow-ownership-moved/,
  );

  assert.equal(
    pullReads,
    4,
    'open PR identities must be re-read in both creation orders',
  );
  assert.equal(
    changedFileReads,
    2,
    'semantic workflow ownership must be re-read even when PR number and head SHA are unchanged',
  );
});

test('shared exact heads reuse one workflow tree while preserving each PR changed-file ownership', async () => {
  const secondWorkflowPath = '.github/workflows/pr-second.yml';
  const pullRequests = [
    {
      number: 42,
      head: { sha: headSha, repo: { full_name: repository } },
    },
    {
      number: 43,
      head: { sha: headSha, repo: { full_name: repository } },
    },
  ];
  let treeReads = 0;

  const paths = await activePullRequestWorkflowPaths(async (url) => {
    if (url.includes(`/git/trees/${headSha}`)) {
      treeReads += 1;
      return {
        truncated: false,
        tree: [
          { type: 'blob', path: workflowPath },
          { type: 'blob', path: secondWorkflowPath },
        ],
      };
    }
    if (url.includes('/pulls/42/files')) {
      return [{ filename: workflowPath, status: 'added' }];
    }
    if (url.includes('/pulls/43/files')) {
      return [{ filename: secondWorkflowPath, status: 'modified' }];
    }
    throw new Error(`unexpected URL ${url}`);
  }, repository, pullRequests);

  assert.equal(treeReads, 1, 'one immutable head tree should be reused across PR identities');
  assert.deepEqual([...paths].sort(), [secondWorkflowPath, workflowPath].sort());
});

test('fails closed when a changed workflow file has an unknown GitHub status', async () => {
  const pullRequests = [{
    number: 42,
    head: { sha: headSha, repo: { full_name: repository } },
  }];

  await assert.rejects(
    activePullRequestWorkflowPaths(async (url) => {
      if (url.includes(`/git/trees/${headSha}`)) {
        return {
          truncated: false,
          tree: [{ type: 'blob', path: workflowPath }],
        };
      }
      if (url.includes('/pulls/42/files')) {
        return [{ filename: workflowPath, status: 'moved' }];
      }
      throw new Error(`unexpected URL ${url}`);
    }, repository, pullRequests),
    /open-pr-file-invalid/,
  );
});

test('fails closed when a changed workflow file status is not a string', async () => {
  const pullRequests = [{
    number: 42,
    head: { sha: headSha, repo: { full_name: repository } },
  }];

  await assert.rejects(
    activePullRequestWorkflowPaths(async (url) => {
      if (url.includes(`/git/trees/${headSha}`)) {
        return {
          truncated: false,
          tree: [{ type: 'blob', path: workflowPath }],
        };
      }
      if (url.includes('/pulls/42/files')) {
        return [{ filename: workflowPath, status: null }];
      }
      throw new Error(`unexpected URL ${url}`);
    }, repository, pullRequests),
    /open-pr-file-invalid/,
  );
});

test('fails closed when a changed-file filename contains parent traversal', async () => {
  const pullRequests = [{
    number: 42,
    head: { sha: headSha, repo: { full_name: repository } },
  }];

  await assert.rejects(
    activePullRequestWorkflowPaths(async (url) => {
      if (url.includes(`/git/trees/${headSha}`)) {
        return {
          truncated: false,
          tree: [{ type: 'blob', path: workflowPath }],
        };
      }
      if (url.includes('/pulls/42/files')) {
        return [{
          filename: '.github/workflows/../workflows/pr-only.yml',
          status: 'modified',
        }];
      }
      throw new Error(`unexpected URL ${url}`);
    }, repository, pullRequests),
    /open-pr-file-invalid/,
  );
});
