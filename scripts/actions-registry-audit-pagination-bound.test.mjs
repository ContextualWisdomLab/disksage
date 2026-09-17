import test from 'node:test';
import assert from 'node:assert/strict';
import {
  listAllOpenPullRequests,
  listAllWorkflowRecords,
} from './actions-registry-audit.mjs';

const repo = 'ContextualWisdomLab/disksage';

test('open PR pagination fails closed before pathological full pages can loop indefinitely', async () => {
  let calls = 0;
  const fullPage = Array.from({ length: 100 }, (_, index) => ({ number: index + 1 }));
  const fetchJson = async () => {
    calls += 1;
    if (calls > 101) throw new Error('sentinel-unbounded-pagination');
    return fullPage;
  };

  await assert.rejects(
    listAllOpenPullRequests(fetchJson, repo),
    /actions-list-page-limit-exceeded/,
  );
  assert.ok(calls <= 100, `expected bounded pagination, observed ${calls} requests`);
});

test('open PR pagination rejects malformed pull request identities', async () => {
  for (const number of [0, '1']) {
    const fetchJson = async () => [{ number }];
    await assert.rejects(
      listAllOpenPullRequests(fetchJson, repo),
      /open-pr-number-invalid/,
    );
  }
});

test('open PR pagination rejects duplicate identities before active workflow ownership can be undercounted', async () => {
  const firstPage = Array.from({ length: 100 }, (_, index) => ({ number: index + 1 }));
  const secondPage = [{ number: 100 }, { number: 101 }];
  const fetchJson = async (pathname) =>
    /[?&]page=1$/.test(pathname) ? firstPage : secondPage;

  await assert.rejects(
    listAllOpenPullRequests(fetchJson, repo),
    /open-pr-list-duplicate/,
  );
});

test('open PR pagination rejects membership drift across opposite created-order reads', async () => {
  const ascendingFirst = Array.from({ length: 100 }, (_, index) => ({ number: index + 1 }));
  const ascendingSecond = [{ number: 102 }];
  const descendingFirst = Array.from({ length: 100 }, (_, index) => ({ number: 102 - index }));
  const descendingSecond = [{ number: 2 }, { number: 1 }];
  const fetchJson = async (pathname) => {
    const pageTwo = /[?&]page=2$/.test(pathname);
    if (pathname.includes('direction=asc')) return pageTwo ? ascendingSecond : ascendingFirst;
    if (pathname.includes('direction=desc')) return pageTwo ? descendingSecond : descendingFirst;
    // The predecessor one-pass implementation has no direction binding and therefore silently
    // accepts this incomplete snapshot instead of detecting membership drift.
    return [{ number: 1 }];
  };

  await assert.rejects(
    listAllOpenPullRequests(fetchJson, repo),
    /open-pr-list-moved/,
  );
});

test('open PR pagination accepts identical membership returned in opposite created order', async () => {
  const fetchJson = async (pathname) => pathname.includes('direction=asc')
    ? [{ number: 1 }, { number: 2 }]
    : [{ number: 2 }, { number: 1 }];

  assert.deepEqual(
    await listAllOpenPullRequests(fetchJson, repo),
    [{ number: 1 }, { number: 2 }],
  );
});

test('workflow registry pagination rejects an externally claimed total beyond the read budget', async () => {
  let calls = 0;
  const fullPage = Array.from({ length: 100 }, (_, index) => ({
    id: index + 1,
    state: 'active',
    path: `dynamic/test-${index + 1}`,
  }));
  const fetchJson = async () => {
    calls += 1;
    if (calls > 101) throw new Error('sentinel-unbounded-pagination');
    return { total_count: 100_001, workflows: fullPage };
  };

  await assert.rejects(
    listAllWorkflowRecords(fetchJson, repo),
    /actions-list-page-limit-exceeded/,
  );
  assert.ok(calls <= 100, `expected bounded workflow pagination, observed ${calls} requests`);
});

test('workflow registry pagination rejects duplicate workflow identities before completeness can be forged', async () => {
  const repeated = {
    id: 77,
    state: 'active',
    path: '.github/workflows/current.yml',
  };
  const fetchJson = async () => ({
    total_count: 2,
    workflows: [repeated, { ...repeated }],
  });

  await assert.rejects(
    listAllWorkflowRecords(fetchJson, repo),
    /actions-workflow-list-duplicate/,
  );
});

test('workflow registry pagination rejects malformed workflow identities at the pagination boundary', async () => {
  for (const workflow of [
    { id: 0, state: 'active', path: '.github/workflows/current.yml' },
    { id: '1', state: 'active', path: '.github/workflows/current.yml' },
    null,
  ]) {
    const fetchJson = async () => ({
      total_count: 1,
      workflows: [workflow],
    });

    await assert.rejects(
      listAllWorkflowRecords(fetchJson, repo),
      /actions-workflow-record-invalid/,
    );
  }
});

test('workflow registry pagination rejects records beyond a stale under-reported total_count', async () => {
  const firstPage = Array.from({ length: 100 }, (_, index) => ({
    id: index + 1,
    state: 'active',
    path: `.github/workflows/workflow-${index + 1}.yml`,
  }));
  const secondPage = [{
    id: 101,
    state: 'active',
    path: '.github/workflows/workflow-101.yml',
  }];
  const fetchJson = async (pathname) => ({
    total_count: 100,
    workflows: /[?&]page=1$/.test(pathname) ? firstPage : secondPage,
  });

  await assert.rejects(
    listAllWorkflowRecords(fetchJson, repo),
    /actions-workflow-list-incomplete/,
  );
});
