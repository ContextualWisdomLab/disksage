import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/** Read one UTF-8 file from the source-controlled repository root. */
function readRepositoryFile(relativePath: string): string {
  return readFileSync(resolve(repositoryRoot, relativePath), 'utf8');
}

/** Return one top-level GitHub Actions job block after normalizing line endings. */
function extractWorkflowJob(workflow: string, jobName: string): string {
  const normalizedWorkflow = workflow.replace(/\r\n?/g, '\n');
  const marker = `\n  ${jobName}:\n`;
  const start = normalizedWorkflow.indexOf(marker);
  if (start < 0) throw new Error(`Missing workflow job: ${jobName}`);
  const remaining = normalizedWorkflow.slice(start + marker.length);
  const nextJobOffset = remaining.search(/\n  [a-zA-Z0-9_-]+:\n/);
  return nextJobOffset < 0 ? remaining : remaining.slice(0, nextJobOffset);
}

/** Extract the literal Bash body from one named workflow step. */
function extractWorkflowRunScript(job: string, stepName: string): string {
  const normalizedJob = job.replace(/\r\n?/g, '\n');
  const stepMarker = `      - name: ${stepName}\n`;
  const stepStart = normalizedJob.indexOf(stepMarker);
  if (stepStart < 0) throw new Error(`Missing workflow step: ${stepName}`);
  const runMarker = '        run: |\n';
  const runStart = normalizedJob.indexOf(runMarker, stepStart);
  if (runStart < 0) throw new Error(`Missing literal run block: ${stepName}`);
  const remaining = normalizedJob.slice(runStart + runMarker.length);
  const nextStepOffset = remaining.search(/\n      - (?:name:|uses:)/);
  const script = nextStepOffset < 0 ? remaining : remaining.slice(0, nextStepOffset);
  return script
    .split('\n')
    .map((line) => (line.startsWith('          ') ? line.slice(10) : line))
    .join('\n');
}

/** Create one minimal repository fixture whose three release versions agree. */
function createVersionFixture(): string {
  const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-version-'));
  mkdirSync(join(fixtureRoot, 'src-tauri'), { recursive: true });
  writeFileSync(
    join(fixtureRoot, 'package.json'),
    JSON.stringify({ name: 'disksage', version: '0.1.0' }),
  );
  writeFileSync(
    join(fixtureRoot, 'src-tauri', 'Cargo.toml'),
    '[package]\nname = "disksage"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\n',
  );
  writeFileSync(
    join(fixtureRoot, 'src-tauri', 'tauri.conf.json'),
    JSON.stringify({ productName: 'DiskSage', version: '0.1.0' }),
  );
  return fixtureRoot;
}

/** Execute the exact release-version admission script against one fixture. */
function runReleaseVersionVerifier(
  fixtureRoot: string,
  ref: string,
  refName: string,
) {
  const workflow = readRepositoryFile('.github/workflows/release.yml');
  const buildJob = extractWorkflowJob(workflow, 'build');
  const verifier = extractWorkflowRunScript(
    buildJob,
    'Verify release version contract',
  );
  return spawnSync('bash', ['-c', verifier], {
    cwd: fixtureRoot,
    encoding: 'utf8',
    env: {
      ...process.env,
      GITHUB_REF: ref,
      GITHUB_REF_NAME: refName,
    },
  });
}

describe('release version contract', () => {
  it.runIf(process.platform !== 'win32')(
    'accepts a tag that exactly matches all release manifests',
    () => {
      const fixtureRoot = createVersionFixture();
      try {
        const result = runReleaseVersionVerifier(
          fixtureRoot,
          'refs/tags/v0.1.0',
          'v0.1.0',
        );

        expect(result.status).toBe(0);
        expect(result.stdout).toContain(
          'Release version contract passed for 0.1.0.',
        );
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );

  it.runIf(process.platform !== 'win32')(
    'rejects a release tag that disagrees with the packaged version',
    () => {
      const fixtureRoot = createVersionFixture();
      try {
        const result = runReleaseVersionVerifier(
          fixtureRoot,
          'refs/tags/v0.2.0',
          'v0.2.0',
        );

        expect(result.status).not.toBe(0);
        expect(result.stderr).toContain(
          'Release tag v0.2.0 does not match manifest version v0.1.0.',
        );
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );

  it.runIf(process.platform !== 'win32')(
    'rejects disagreement between package, Cargo, and Tauri versions',
    () => {
      const fixtureRoot = createVersionFixture();
      try {
        writeFileSync(
          join(fixtureRoot, 'src-tauri', 'Cargo.toml'),
          '[package]\nname = "disksage"\nversion = "0.2.0"\nedition = "2021"\n',
        );

        const result = runReleaseVersionVerifier(
          fixtureRoot,
          'refs/heads/main',
          'main',
        );

        expect(result.status).not.toBe(0);
        expect(result.stderr).toContain(
          'Release manifest versions disagree: package.json=0.1.0, Cargo.toml=0.2.0, tauri.conf.json=0.1.0.',
        );
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );
});
