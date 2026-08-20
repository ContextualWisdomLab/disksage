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
