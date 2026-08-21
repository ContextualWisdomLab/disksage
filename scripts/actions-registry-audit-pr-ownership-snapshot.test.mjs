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

  assert.equal(pullReads, 2, 'open PR identities must be re-read');
  assert.equal(
    changedFileReads,
    2,
    'semantic workflow ownership must be re-read even when PR number and head SHA are unchanged',
  );
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
