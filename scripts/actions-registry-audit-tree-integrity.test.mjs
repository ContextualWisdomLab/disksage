import test from 'node:test';
import assert from 'node:assert/strict';

import {
  activePullRequestWorkflowPaths,
  protectedWorkflowPaths,
} from './actions-registry-audit.mjs';

const repository = 'ContextualWisdomLab/disksage';
const protectedMainSha = 'a'.repeat(40);
const pullHeadSha = 'b'.repeat(40);

function protectedMainFetch(tree) {
  return async (url) => {
    if (url.endsWith('/disksage')) return { default_branch: 'main' };
    if (url.endsWith('/commits/main')) return { sha: protectedMainSha };
    if (url.includes(`/git/trees/${protectedMainSha}`)) return tree;
    throw new Error(`unexpected URL ${url}`);
  };
}

test('protected-main workflow discovery rejects unsafe normalized blob identity evidence', async () => {
  await assert.rejects(
    protectedWorkflowPaths(
      protectedMainFetch({
        truncated: false,
        tree: [
          { type: 'blob', path: '.github/workflows/test.yml' },
          { type: 'blob', path: '../.github/workflows/escape.yml' },
        ],
      }),
      repository,
      protectedMainSha,
    ),
    /protected-main-tree-entry-invalid/,
  );
});

test('protected-main workflow discovery ignores empty blob paths without granting authority', async () => {
  const paths = await protectedWorkflowPaths(
    protectedMainFetch({
      truncated: false,
      tree: [
        { type: 'blob', path: '' },
        { type: 'blob', path: '.github/workflows/test.yml' },
      ],
    }),
    repository,
    protectedMainSha,
  );

  assert.deepEqual([...paths], ['.github/workflows/test.yml']);
});

test('protected-main workflow discovery rejects duplicate normalized workflow identities', async () => {
  await assert.rejects(
    protectedWorkflowPaths(
      protectedMainFetch({
        truncated: false,
        tree: [
          { type: 'blob', path: '.github/workflows/test.yml' },
          { type: 'blob', path: './.github/workflows/test.yml' },
        ],
      }),
      repository,
      protectedMainSha,
    ),
    /protected-main-tree-entry-invalid/,
  );
});

test('protected-main workflow discovery rejects symlink-mode workflow blobs', async () => {
  await assert.rejects(
    protectedWorkflowPaths(
      protectedMainFetch({
        truncated: false,
        tree: [
          { type: 'blob', mode: '120000', path: '.github/workflows/test.yml' },
        ],
      }),
      repository,
      protectedMainSha,
    ),
    /protected-main-tree-entry-invalid/,
  );
});

test('active-PR workflow discovery rejects unsafe exact-head blob identity evidence', async () => {
  const pullRequests = [
    {
      number: 192,
      head: {
        sha: pullHeadSha,
        repo: { full_name: repository },
      },
    },
  ];

  await assert.rejects(
    activePullRequestWorkflowPaths(async (url) => {
      if (url.includes(`/git/trees/${pullHeadSha}`)) {
        return {
          truncated: false,
          tree: [
            { type: 'blob', path: '.github/workflows/actions-registry-audit.yml' },
            { type: 'blob', path: '../.github/workflows/escape.yml' },
          ],
        };
      }
      if (url.includes('/pulls/192/files')) {
        return [
          { filename: '.github/workflows/actions-registry-audit.yml', status: 'modified' },
        ];
      }
      throw new Error(`unexpected URL ${url}`);
    }, repository, pullRequests),
    /workflow-tree-entry-invalid/,
  );
});

test('active-PR workflow discovery rejects symlink-mode exact-head workflow blobs', async () => {
  const pullRequests = [
    {
      number: 192,
      head: {
        sha: pullHeadSha,
        repo: { full_name: repository },
      },
    },
  ];

  await assert.rejects(
    activePullRequestWorkflowPaths(async (url) => {
      if (url.includes(`/git/trees/${pullHeadSha}`)) {
        return {
          truncated: false,
          tree: [
            {
              type: 'blob',
              mode: '120000',
              path: '.github/workflows/actions-registry-audit.yml',
            },
          ],
        };
      }
      if (url.includes('/pulls/192/files')) {
        return [
          { filename: '.github/workflows/actions-registry-audit.yml', status: 'modified' },
        ];
      }
      throw new Error(`unexpected URL ${url}`);
    }, repository, pullRequests),
    /workflow-tree-entry-invalid/,
  );
});
