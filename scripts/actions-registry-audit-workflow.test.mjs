import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const workflow = readFileSync(
  new URL('../.github/workflows/actions-registry-audit.yml', import.meta.url),
  'utf8',
);

function jobSection(name) {
  const start = new RegExp(`^  ${name}:\\n`, 'm').exec(workflow);
  assert.ok(start, `workflow must define the ${name} job`);
  const bodyStart = start.index + start[0].length;
  const nextJob = /^  [A-Za-z0-9_-]+:\n/gm;
  nextJob.lastIndex = bodyStart;
  const next = nextJob.exec(workflow);
  return workflow.slice(start.index, next?.index ?? workflow.length);
}

const contractsJob = jobSection('contracts');
const liveRegistryJob = jobSection('live-registry');

test('native coverage thresholds run on a Node release that implements every required flag', () => {
  for (const flag of [
    "--test-coverage-include='scripts/actions-registry-audit.mjs'",
    '--test-coverage-lines=100',
    '--test-coverage-branches=100',
    '--test-coverage-functions=100',
  ]) {
    assert.match(contractsJob, new RegExp(flag.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  }

  const versionMatch = contractsJob.match(/node-version:\s+(\d+)\.(\d+)\.(\d+)/);
  assert.ok(versionMatch, 'the contracts job must pin an exact Node release');
  const major = Number(versionMatch[1]);
  const minor = Number(versionMatch[2]);
  assert.ok(
    major > 22 || (major === 22 && minor >= 8),
    'coverage include and threshold flags require Node 22.8.0 or newer',
  );
});

test('live registry uses the tested Node runtime and bounded transient GitHub API retries', () => {
  const contractsVersion = contractsJob.match(/node-version:\s+(\d+\.\d+\.\d+)/)?.[1];
  const liveVersion = liveRegistryJob.match(/node-version:\s+(\d+\.\d+\.\d+)/)?.[1];
  assert.ok(contractsVersion && liveVersion, 'both jobs must pin exact Node releases');
  assert.equal(liveVersion, contractsVersion, 'live audit runtime must match the tested contracts runtime');
  assert.match(liveRegistryJob, /AbortSignal\.timeout\(15_000\)/);
  assert.match(liveRegistryJob, /response\.status === 429 \|\| response\.status >= 500/);
  assert.match(liveRegistryJob, /attempt >= 3/);
});
