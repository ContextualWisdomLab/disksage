import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const workflow = readFileSync(
  new URL('../.github/workflows/actions-registry-audit.yml', import.meta.url),
  'utf8',
);
const contractsJob = workflow.split('\n  live-registry:')[0];

test('native coverage thresholds run on a Node release that implements every required flag', () => {
  for (const flag of [
    '--test-coverage-include=',
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
