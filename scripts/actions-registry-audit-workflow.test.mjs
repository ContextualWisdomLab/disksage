import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const workflow = readFileSync(
  new URL('../.github/workflows/actions-registry-audit.yml', import.meta.url),
  'utf8',
);

function jobSection(name) {
  const marker = `  ${name}:\n`;
  const startIndex = workflow.indexOf(marker);
  assert.notEqual(startIndex, -1, `workflow must define the ${name} job`);
  const bodyStart = startIndex + marker.length;
  const tail = workflow.slice(bodyStart);
  let offset = 0;
  for (const line of tail.split('\n')) {
    if (
      line.startsWith('  ') &&
      !line.startsWith('    ') &&
      line.endsWith(':') &&
      line.length > 3
    ) {
      return workflow.slice(startIndex, bodyStart + offset);
    }
    offset += line.length + 1;
  }
  return workflow.slice(startIndex);
}

const contractsJob = jobSection('contracts');
const liveRegistryJob = jobSection('live-registry');

test('contracts checkout binds pull-request evidence to the immutable source head', () => {
  assert.match(
    contractsJob,
    /ref:\s+\$\{\{\s*github\.event\.pull_request\.head\.sha\s*\|\|\s*github\.sha\s*\}\}/,
    'PR contracts must checkout the exact source head instead of GitHub synthetic merge ref',
  );
  assert.match(contractsJob, /persist-credentials:\s+false/);
});

test('native coverage thresholds run on a Node release that implements every required flag', () => {
  for (const flag of [
    "--test-coverage-include='scripts/actions-registry-audit.mjs'",
    '--test-coverage-lines=100',
    '--test-coverage-branches=100',
    '--test-coverage-functions=100',
  ]) {
    assert.ok(contractsJob.includes(flag), `contracts job must contain ${flag}`);
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

test('coverage diagnostics cannot mask the authoritative coverage exit status', () => {
  const coverageStatus = contractsJob.indexOf('coverage_status=$?');
  const diagnosticStart = contractsJob.indexOf("node --input-type=module <<'NODE'");
  const diagnosticStatus = contractsJob.indexOf('diagnostic_status=$?');
  const strictModeRestore = contractsJob.indexOf('set -e', diagnosticStart);
  const finalExit = contractsJob.indexOf('exit "$coverage_status"');

  assert.ok(coverageStatus >= 0, 'contracts must capture the primary coverage status');
  assert.ok(diagnosticStart > coverageStatus, 'diagnostics must run only after coverage status is captured');
  assert.ok(
    diagnosticStatus > diagnosticStart,
    'diagnostic failure must be captured instead of aborting the step under set -e',
  );
  assert.ok(
    strictModeRestore > diagnosticStatus,
    'strict shell mode must be restored only after diagnostic status is captured',
  );
  assert.ok(finalExit > strictModeRestore, 'the step must finish with the original coverage status');
  assert.match(
    contractsJob,
    /if \[ "\$diagnostic_status" -ne 0 \]; then[\s\S]*::warning::actions registry coverage diagnostic failed/,
    'diagnostic failure should remain visible without replacing the authoritative coverage failure',
  );
});

test('PR files cap regression is both workflow-triggering and executed by exact audit coverage', () => {
  const regression = 'scripts/actions-registry-audit-pr-files-cap.test.mjs';
  const occurrences = workflow.split(regression).length - 1;
  assert.ok(
    occurrences >= 3,
    'the PR-files cap regression must trigger pull/push runs and execute in contracts coverage',
  );
  assert.ok(
    contractsJob.includes(regression),
    'exact audit-module coverage must execute the PR-files cap regression',
  );
});

test('PR ownership snapshot regression is both workflow-triggering and executed by exact audit coverage', () => {
  const regression = 'scripts/actions-registry-audit-pr-ownership-snapshot.test.mjs';
  const occurrences = workflow.split(regression).length - 1;
  assert.ok(
    occurrences >= 3,
    'the PR ownership regression must trigger pull/push runs and execute in contracts coverage',
  );
  assert.ok(
    contractsJob.includes(regression),
    'exact audit-module coverage must execute the PR ownership snapshot regression',
  );
});

test('live registry uses the tested Node runtime and bounded transient GitHub API retries', () => {
  const contractsVersion = contractsJob.match(/node-version:\s+(\d+\.\d+\.\d+)/)?.[1];
  const liveVersion = liveRegistryJob.match(/node-version:\s+(\d+\.\d+\.\d+)/)?.[1];
  assert.ok(contractsVersion && liveVersion, 'both jobs must pin exact Node releases');
  assert.equal(liveVersion, contractsVersion, 'live audit runtime must match the tested contracts runtime');
  assert.match(liveRegistryJob, /AbortSignal\.timeout\(15_000\)/);
  assert.match(liveRegistryJob, /response\.status === 429 \|\| response\.status >= 500/);
  assert.match(
    liveRegistryJob,
    /for \(let attempt = 1; attempt <= 3; attempt \+= 1\)/,
  );
  assert.match(liveRegistryJob, /attempt >= 3/);
  assert.doesNotMatch(
    liveRegistryJob,
    /github-api-retry-exhausted/,
    'bounded retry control flow must not retain an unreachable post-loop failure path',
  );
});

test('live registry binds executed audit code and protected-main evidence to one immutable SHA', () => {
  assert.match(
    liveRegistryJob,
    /ref:\s+\$\{\{\s*github\.sha\s*\}\}/,
    'live audit must execute the exact triggering protected-main source revision',
  );
  assert.match(liveRegistryJob, /persist-credentials:\s+false/);
  assert.match(
    liveRegistryJob,
    /EXPECTED_MAIN_SHA:\s+\$\{\{\s*github\.sha\s*\}\}/,
    'the same immutable source revision must be passed into protected-main drift validation',
  );
  assert.match(
    liveRegistryJob,
    /auditActionsRegistry\(fetchJson, repository, expectedMainSha\)/,
  );
  assert.doesNotMatch(
    liveRegistryJob,
    /commits\/\$\{encodeURIComponent\(metadata\.default_branch\)\}/,
    'live audit must not silently retarget evidence to a newer main than the executed audit code',
  );
});
