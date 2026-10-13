import test from 'node:test';
import assert from 'node:assert/strict';
import { activePullRequestWorkflowPaths } from './actions-registry-audit.mjs';

const repository = 'ContextualWisdomLab/disksage';
const headSha = 'a'.repeat(40);
const pullNumber = 4242;
const cappedWorkflowPath = '.github/workflows/owned-beyond-api-cap.yml';

function changedFile(page, index) {
  return {
    filename: `src/capped-page-${page}/file-${index}.rs`,
    status: 'modified',
  };
}

test('fails closed at the GitHub REST 3000-file cap without requesting an unsupported page', async () => {
  const pullRequests = [{
    number: pullNumber,
    head: { sha: headSha, repo: { full_name: repository } },
  }];
  let filePageReads = 0;

  await assert.rejects(
    activePullRequestWorkflowPaths(async (url) => {
      if (url.includes(`/git/trees/${headSha}`)) {
        return {
          truncated: false,
          tree: [{ type: 'blob', path: cappedWorkflowPath }],
        };
      }
      if (url.includes(`/pulls/${pullNumber}/files`)) {
        filePageReads += 1;
        const match = /[?&]page=(\d+)/.exec(url);
        const page = Number(match?.[1]);
        if (page >= 1 && page <= 30) {
          return Array.from({ length: 100 }, (_, index) => changedFile(page, index));
        }
        throw new Error(`unsupported PR-files page ${page}`);
      }
      throw new Error(`unexpected URL ${url}`);
    }, repository, pullRequests),
    /open-pr-files-limit-exceeded/,
  );

  assert.equal(
    filePageReads,
    30,
    'the audit must fail at 3000 records instead of depending on an unavailable page 31 response',
  );
});

test('fails closed on duplicate changed-file identities instead of accepting a shifted pagination snapshot', async () => {
  const workflowPath = '.github/workflows/owned-by-pr.yml';
  const pullRequests = [{
    number: pullNumber,
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
      if (url.includes(`/pulls/${pullNumber}/files`)) {
        return [
          { filename: workflowPath, status: 'added' },
          { filename: workflowPath, status: 'added' },
        ];
      }
      throw new Error(`unexpected URL ${url}`);
    }, repository, pullRequests),
    /open-pr-files-duplicate/,
  );
});
