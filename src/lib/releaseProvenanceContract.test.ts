import { createHash } from 'node:crypto';
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const operationalAssetNames = [
  'disksage-cloud-plan-linux-x86_64',
  'disksage-duplicate-audit-linux-x86_64',
  'disksage-cloud-plan-windows-x86_64.exe',
  'disksage-duplicate-audit-windows-x86_64.exe',
  'disksage-cloud-plan-macos-arm64',
  'disksage-duplicate-audit-macos-arm64',
] as const;

/**
 * Read one UTF-8 repository file from the source-controlled project root.
 *
 * Resolving from this module keeps the governance contract deterministic when
 * Vitest runs from an IDE, a parent workspace, or an isolated CI directory.
 */
function readRepositoryFile(relativePath: string): string {
  return readFileSync(resolve(repositoryRoot, relativePath), 'utf8');
}

/**
 * Return one top-level GitHub Actions job block without parsing untrusted YAML.
 *
 * Release governance tests need only stable job boundaries. Normalizing CRLF
 * and legacy CR separators keeps the contract portable across Git checkouts on
 * Windows, macOS, and Linux without changing the source-controlled workflow.
 */
function extractWorkflowJob(workflow: string, jobName: string): string {
  const normalizedWorkflow = workflow.replace(/\r\n?/g, '\n');
  const marker = `\n  ${jobName}:\n`;
  const start = normalizedWorkflow.indexOf(marker);
  if (start < 0) {
    throw new Error(`Missing workflow job: ${jobName}`);
  }

  const contentStart = start + marker.length;
  const remaining = normalizedWorkflow.slice(contentStart);
  const nextJobOffset = remaining.search(/\n  [a-zA-Z0-9_-]+:\n/);
  return nextJobOffset < 0 ? remaining : remaining.slice(0, nextJobOffset);
}

/**
 * Extract one literal Bash `run` block from a named workflow step.
 *
 * The release validator is security-sensitive executable policy. Running the
 * exact source-controlled block against fixtures proves behavior without
 * maintaining a second, test-only implementation that could drift.
 */
function extractWorkflowRunScript(job: string, stepName: string): string {
  const normalizedJob = job.replace(/\r\n?/g, '\n');
  const stepMarker = `      - name: ${stepName}\n`;
  const stepStart = normalizedJob.indexOf(stepMarker);
  if (stepStart < 0) {
    throw new Error(`Missing workflow step: ${stepName}`);
  }

  const runMarker = '        run: |\n';
  const runStart = normalizedJob.indexOf(runMarker, stepStart);
  if (runStart < 0) {
    throw new Error(`Missing literal run block for workflow step: ${stepName}`);
  }

  const scriptStart = runStart + runMarker.length;
  const remaining = normalizedJob.slice(scriptStart);
  const nextStepOffset = remaining.search(/\n      - (?:name:|uses:)/);
  const indentedScript =
    nextStepOffset < 0 ? remaining : remaining.slice(0, nextStepOffset);
  return indentedScript
    .split('\n')
    .map((line) => (line.startsWith('          ') ? line.slice(10) : line))
    .join('\n');
}

/**
 * Write a realistic complete release-artifact tree and return its root.
 *
 * Each operational checksum initially names and authenticates the adjacent CLI
 * exactly as the platform build job does. Individual tests can then mutate one
 * boundary while every unrelated admission requirement remains valid.
 */
function createReleaseArtifactFixture(): string {
  const fixtureRoot = mkdtempSync(join(tmpdir(), 'disksage-release-contract-'));
  const artifactRoot = join(fixtureRoot, 'release-artifacts');
  const bundlePaths = [
    'ubuntu/bundle/deb/disksage.deb',
    'ubuntu/bundle/appimage/disksage.AppImage',
    'windows/bundle/msi/disksage.msi',
    'windows/bundle/nsis/disksage-setup.exe',
    'macos/bundle/dmg/disksage.dmg',
  ];

  for (const bundlePath of bundlePaths) {
    const absolutePath = join(artifactRoot, bundlePath);
    mkdirSync(dirname(absolutePath), { recursive: true });
    writeFileSync(absolutePath, `bundle:${bundlePath}`);
  }

  for (const assetName of operationalAssetNames) {
    const platformDirectory = assetName.includes('windows')
      ? 'windows'
      : assetName.includes('macos')
        ? 'macos'
        : 'ubuntu';
    const assetPath = join(artifactRoot, platformDirectory, assetName);
    const assetBytes = Buffer.from(`operational-cli:${assetName}`);
    mkdirSync(dirname(assetPath), { recursive: true });
    writeFileSync(assetPath, assetBytes);
    writeFileSync(
      `${assetPath}.sha256`,
      `${createHash('sha256').update(assetBytes).digest('hex')}  ${assetName}\n`,
    );
  }

  return fixtureRoot;
}

/** Execute the exact release-admission Bash block in one fixture directory. */
function runReleaseArtifactVerifier(fixtureRoot: string) {
  const workflow = readRepositoryFile('.github/workflows/release.yml');
  const attestJob = extractWorkflowJob(workflow, 'attest-release');
  const verifier = extractWorkflowRunScript(
    attestJob,
    'Verify release artifact checksums',
  );
  return spawnSync('bash', ['-c', verifier], {
    cwd: fixtureRoot,
    encoding: 'utf8',
  });
}

describe('release artifact provenance contract', () => {
  it('extracts workflow jobs from Windows CRLF checkouts', () => {
    const workflow =
      'jobs:\r\n  build:\r\n    runs-on: windows-latest\r\n  publish:\r\n    runs-on: ubuntu-latest\r\n';

    expect(extractWorkflowJob(workflow, 'build')).toContain(
      'runs-on: windows-latest',
    );
    expect(extractWorkflowJob(workflow, 'build')).not.toContain(
      'runs-on: ubuntu-latest',
    );
  });

  it('binds all required test jobs to the exact current head', () => {
    const workflow = readRepositoryFile('.github/workflows/test.yml');
    const exactHeadCheckout =
      'ref: ${{ github.event.pull_request.head.sha || github.sha }}';

    expect(extractWorkflowJob(workflow, 'test')).toContain(exactHeadCheckout);
    expect(extractWorkflowJob(workflow, 'llm-engine-build')).toContain(
      exactHeadCheckout,
    );
  });

  it('makes exact release provenance a tag-only gate before publication', () => {
    const workflow = readRepositoryFile('.github/workflows/release.yml');
    const buildJob = extractWorkflowJob(workflow, 'build');
    const attestJob = extractWorkflowJob(workflow, 'attest-release');
    const publishJob = extractWorkflowJob(workflow, 'publish-release');

    expect(buildJob).toContain('name: Upload release artifact set');
    expect(buildJob).toContain('name: release-disksage-${{ matrix.os }}');
    expect(buildJob).toContain(
      'ref: ${{ github.event.pull_request.head.sha || github.sha }}',
    );
    expect(buildJob).not.toContain('softprops/action-gh-release');

    expect(attestJob).toContain("if: startsWith(github.ref, 'refs/tags/')");
    expect(attestJob).toContain('needs: build');
    expect(attestJob).toContain('contents: read');
    expect(attestJob).toContain('id-token: write');
    expect(attestJob).toContain('attestations: write');
    expect(attestJob).toContain(
      'actions/download-artifact@37930b1c2abaa49bbe596cd826c3c89aef350131',
    );
    expect(attestJob).toContain('pattern: release-disksage-*');
    expect(attestJob).toContain('merge-multiple: false');
    expect(attestJob).not.toContain('merge-multiple: true');
    expect(attestJob).toContain(
      'actions/attest@59d89421af93a897026c735860bf21b6eb4f7b26',
    );
    expect(attestJob).toContain('subject-path: release-artifacts/**/*');
    expect(attestJob).toContain('name: Verify release artifact checksums');
    expect(attestJob).toContain('require_exactly_one_file()');
    expect(attestJob).not.toContain('require_file()');
    expect(attestJob).toContain(
      'require_exactly_one_file "$required_name"',
    );
    expect(attestJob).toContain(
      'require_exactly_one_file "$required_name.sha256"',
    );
    expect(attestJob).toContain(
      "require_exactly_one_path '*/bundle/nsis/*.exe' 'Windows NSIS bundle'",
    );
    expect(attestJob).not.toContain("require_exactly_one '*.exe'");
    expect(
      attestJob.indexOf('name: Verify release artifact checksums'),
    ).toBeLessThan(
      attestJob.indexOf(
        'actions/attest@59d89421af93a897026c735860bf21b6eb4f7b26',
      ),
    );

    expect(publishJob).toContain("if: startsWith(github.ref, 'refs/tags/')");
    expect(publishJob).toContain('needs: attest-release');
    expect(publishJob).toContain('contents: write');
    expect(publishJob).toContain(
      'actions/download-artifact@37930b1c2abaa49bbe596cd826c3c89aef350131',
    );
    expect(publishJob).toContain('pattern: release-disksage-*');
    expect(publishJob).toContain('merge-multiple: false');
    expect(publishJob).not.toContain('merge-multiple: true');
    expect(publishJob).toContain(
      'softprops/action-gh-release@3d0d9888cb7fd7b750713d6e236d1fcb99157228',
    );
  });

  it.runIf(process.platform !== 'win32')(
    'rejects a checksum record that authenticates a decoy instead of the expected CLI',
    () => {
      const fixtureRoot = createReleaseArtifactFixture();
      try {
        const linuxDirectory = join(
          fixtureRoot,
          'release-artifacts',
          'ubuntu',
        );
        const checksumPath = join(
          linuxDirectory,
          'disksage-cloud-plan-linux-x86_64.sha256',
        );
        const decoyName = 'unpublished-decoy';
        const decoyBytes = Buffer.from('not-the-published-cli');
        writeFileSync(join(linuxDirectory, decoyName), decoyBytes);
        writeFileSync(
          checksumPath,
          `${createHash('sha256').update(decoyBytes).digest('hex')}  ${decoyName}\n`,
        );

        const result = runReleaseArtifactVerifier(fixtureRoot);

        expect(result.status).not.toBe(0);
        expect(result.stderr).toContain(
          'must reference its adjacent operational CLI',
        );
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );

  it.runIf(process.platform !== 'win32')(
    'rejects one required CLI duplicated across preserved artifact namespaces',
    () => {
      const fixtureRoot = createReleaseArtifactFixture();
      try {
        const duplicatedName = 'disksage-cloud-plan-linux-x86_64';
        const sourcePath = join(
          fixtureRoot,
          'release-artifacts',
          'ubuntu',
          duplicatedName,
        );
        const duplicatePath = join(
          fixtureRoot,
          'release-artifacts',
          'windows',
          duplicatedName,
        );
        mkdirSync(dirname(duplicatePath), { recursive: true });
        writeFileSync(duplicatePath, readFileSync(sourcePath));

        const result = runReleaseArtifactVerifier(fixtureRoot);

        expect(result.status).not.toBe(0);
        expect(result.stderr).toContain(
          `Expected exactly one release artifact named ${duplicatedName}, found 2.`,
        );
      } finally {
        rmSync(fixtureRoot, { recursive: true, force: true });
      }
    },
  );

  it('documents buyer-verifiable provenance and authoritative standards', () => {
    const doctoring = readRepositoryFile(
      'docs/doctoring/release-artifact-provenance.md',
    );
    const changelog = readRepositoryFile('CHANGELOG.md');

    expect(doctoring).toContain('# Release artifact provenance');
    expect(doctoring).toContain(
      'gh attestation verify PATH/TO/ARTIFACT -R ContextualWisdomLab/disksage',
    );
    expect(doctoring).toContain('SLSA Provenance v1');
    expect(doctoring).toContain('in-toto Statement v1');
    expect(doctoring).toContain('APA 7th references');
    expect(doctoring).toContain('59d89421af93a897026c735860bf21b6eb4f7b26');
    expect(doctoring).toContain('37930b1c2abaa49bbe596cd826c3c89aef350131');
    expect(doctoring).toContain(
      'operational CLI, or checksum file is absent or duplicated',
    );
    expect(doctoring).toContain(
      'checksum record names a file other than its adjacent operational CLI',
    );
    expect(changelog).toContain('buyer-verifiable release artifact provenance');
  });
});
