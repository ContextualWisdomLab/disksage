import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(import.meta.dirname, '../..');
const assets = [
  ['ubuntu-22.04', 'disksage-cloud-plan-linux-x86_64'],
  ['ubuntu-22.04', 'disksage-duplicate-audit-linux-x86_64'],
  ['windows-2022', 'disksage-cloud-plan-windows-x86_64.exe'],
  ['windows-2022', 'disksage-duplicate-audit-windows-x86_64.exe'],
  ['macos-latest', 'disksage-cloud-plan-macos-arm64'],
  ['macos-latest', 'disksage-duplicate-audit-macos-arm64'],
] as const;
type Program = 'standalone' | 'workflow';
type Backend = 'sha256sum' | 'shasum';

function write(path: string, bytes: string | Buffer) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, bytes);
}

/** Resolve real executables before installing the fixture-only PATH. */
function executable(name: string): string {
  const result = spawnSync('bash', ['-c', 'command -v "$1"', 'lookup', name], {
    encoding: 'utf8', timeout: 5000,
  });
  if (result.error || result.status !== 0 || !result.stdout.trim().startsWith('/')) {
    throw new Error(`Real executable unavailable: ${name}: ${result.stderr}`);
  }
  return result.stdout.trim();
}

function quote(value: string): string {
  return `'${value.replaceAll("'", "'\\''")}'`;
}

/** Only strip YAML indentation: execute the entire actual attestation admission body. */
function workflowProgram(): string {
  const workflow = readFileSync(join(repositoryRoot, '.github/workflows/release.yml'), 'utf8').replace(/\r\n/g, '\n');
  const jobMarker = '\n  attest-release:\n';
  const jobStart = workflow.indexOf(jobMarker);
  if (jobStart < 0) throw new Error('Missing attestation job');
  const job = workflow.slice(jobStart + jobMarker.length).split(/\n  [\w-]+:\n/)[0];
  const marker = '      - name: Verify release artifact checksums\n';
  const stepStart = job.indexOf(marker);
  if (stepStart < 0) throw new Error('Missing admission step');
  const step = job.slice(stepStart).split(/\n      - (?:name:|uses:)/)[0];
  const runMarker = '        run: |\n';
  const runStart = step.indexOf(runMarker);
  if (runStart < 0) throw new Error('Missing literal admission body');
  return step.slice(runStart + runMarker.length).split('\n').map((line) => {
    if (line && !line.startsWith('          ')) throw new Error('Unexpected run indentation');
    return line.slice(10);
  }).join('\n');
}

// Guard before initialization: native Windows must collect without resolving
// POSIX tool paths, even when Git Bash happens to be installed.
const real = process.platform === 'win32' ? undefined : {
  bash: executable('bash'), find: executable('find'),
  dirname: executable('dirname'), basename: executable('basename'),
  sha256sum: executable('sha256sum'), shasum: executable('shasum'),
};

/** Transparent witnesses invoke the real tools, retain their bytes/status, and never invent hashes. */
function installWitnesses(bin: string, backend: Backend) {
  if (!real) throw new Error('Unix executable fixtures are unavailable on Windows');
  mkdirSync(bin, { recursive: true });
  symlinkSync(real.find, join(bin, 'find'));
  symlinkSync(process.execPath, join(bin, 'node'));
  for (const name of ['dirname', 'basename'] as const) {
    write(join(bin, name), `#!/bin/bash\nprintf '%s\\n' '${name}' >>"$PATH_HELPER_LOG"\nexec ${quote(real[name])} "$@"\n`);
  }
  write(join(bin, backend), [
    '#!/bin/bash',
    `${quote(real[backend])} "$@"`,
    'status=$?',
    `printf '%s\\t%s\\t%s\\t%s\\n' '${backend}' "$status" "$PWD" "$*" >>"$SHA_LOG"`,
    'exit "$status"', '',
  ].join('\n'));
  // Executable permissions apply only to owned witnesses, never installed tools.
  for (const name of ['dirname', 'basename', backend]) chmodSync(join(bin, name), 0o755);
}

type Fixture = {
  root: string; artifactRoot: string; cwd: string; rootArgument: string;
  helperLog: string; shaLog: string; bin: string;
};

function withFixture(program: Program, backend: Backend, assertion: (fixture: Fixture) => void) {
  // Bash reports the physical PWD; macOS /var is an alias of /private/var.
  // Canonicalize this newly owned root so exact witness-path checks bind one identity.
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'disksage-path-expansion-')));
  const specialRoot = join(root, '-release space 雪');
  const artifactRoot = join(specialRoot, 'release-artifacts');
  const fixture: Fixture = {
    root, artifactRoot, cwd: program === 'workflow' ? specialRoot : root,
    rootArgument: './-release space 雪/release-artifacts',
    helperLog: join(root, 'helpers.log'), shaLog: join(root, 'sha.log'), bin: join(root, 'bin'),
  };
  try {
    write(fixture.helperLog, '');
    write(fixture.shaLog, '');
    installWitnesses(fixture.bin, backend);
    const bundles = [
      ['ubuntu-22.04', 'deb/disksage.deb'], ['ubuntu-22.04', 'appimage/disksage.AppImage'],
      ['windows-2022', 'msi/disksage.msi'], ['windows-2022', 'nsis/disksage-setup.exe'],
      ['macos-latest', 'dmg/disksage.dmg'],
    ];
    for (const [platform, bundle] of bundles) {
      write(join(artifactRoot, `release-disksage-${platform}-1`, 'bundle', bundle), `bundle:${bundle}`);
    }
    for (const [platform, name] of assets) {
      const path = join(artifactRoot, `release-disksage-${platform}-1`, name);
      const bytes = Buffer.from(`real fixture bytes: ${name}\n雪\0`, 'utf8');
      write(path, bytes);
      write(`${path}.sha256`, `${createHash('sha256').update(bytes).digest('hex')}  ${name}\n`);
    }
    if (program === 'workflow') {
      write(join(artifactRoot, 'sbom/disksage.spdx.json'), JSON.stringify({
        spdxVersion: 'SPDX-2.3', SPDXID: 'SPDXRef-DOCUMENT',
        documentNamespace: 'https://github.com/ContextualWisdomLab/disksage/sbom/deadbeef',
        packages: [{ SPDXID: 'SPDXRef-root', name: 'cargo:disksage', versionInfo: '0.1.0', downloadLocation: 'NOASSERTION' }],
        documentDescribes: ['SPDXRef-root'], relationships: [],
      }));
    }
    assertion(fixture);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function run(program: Program, fixture: Fixture, shell: string) {
  const source = program === 'workflow'
    ? workflowProgram()
    : readFileSync(join(repositoryRoot, '.github/scripts/verify-release-artifacts.sh'), 'utf8');
  return spawnSync(shell, ['-c', source, 'admission', fixture.rootArgument, '1'], {
    cwd: fixture.cwd, encoding: 'utf8', timeout: 5000,
    env: {
      ...process.env, PATH: fixture.bin, PATH_HELPER_LOG: fixture.helperLog, SHA_LOG: fixture.shaLog,
      GITHUB_WORKSPACE: repositoryRoot,
    },
  });
}

function shaRows(fixture: Fixture): string[][] {
  return readFileSync(fixture.shaLog, 'utf8').trim().split('\n').filter(Boolean).map((line) => line.split('\t'));
}

function assertSuccessfulHashes(fixture: Fixture, backend: Backend) {
  const rows = shaRows(fixture);
  expect.soft(rows).toHaveLength(6);
  const checked = rows.map(([tool, status, cwd, args]) => {
    expect.soft(tool).toBe(backend);
    expect.soft(status).toBe('0');
    const prefix = backend === 'sha256sum' ? '--check ' : '-a 256 --check ';
    expect.soft(args.startsWith(prefix)).toBe(true);
    const checksumName = args.slice(prefix.length);
    const checksumPath = join(cwd, checksumName);
    expect.soft(readFileSync(checksumPath, 'utf8')).toMatch(/^[0-9a-f]{64}  [\w.-]+\n$/);
    return checksumName;
  });
  expect.soft(checked.sort()).toEqual(assets.map(([, name]) => `${name}.sha256`).sort());
}

type NegativeCase = {
  name: string; diagnostic: string; mutate: (fixture: Fixture, cli: string, checksum: string) => void;
  realHashFailure?: boolean; allHashes?: boolean; noHashes?: boolean;
};
const negatives: NegativeCase[] = [
  {
    name: 'wrong adjacent owner', diagnostic: 'must reference its adjacent operational CLI',
    mutate: (_fixture, _cli, checksum) => {
      const other = join(dirname(checksum), assets[1][1]);
      const digest = createHash('sha256').update(readFileSync(other)).digest('hex');
      write(checksum, `${digest}  ${assets[1][1]}\n`);
    },
  },
  {
    name: 'decoy ownership', diagnostic: 'must reference its adjacent operational CLI',
    mutate: (_fixture, _cli, checksum) => {
      const bytes = Buffer.from('unpublished decoy bytes');
      write(join(dirname(checksum), 'decoy'), bytes);
      write(checksum, `${createHash('sha256').update(bytes).digest('hex')}  decoy\n`);
    },
  },
  {
    name: 'tampered adjacent bytes', diagnostic: 'FAILED', realHashFailure: true,
    mutate: (_fixture, cli) => write(cli, 'tampered published CLI'),
  },
  {
    name: 'malformed digest', diagnostic: 'must reference its adjacent operational CLI',
    mutate: (_fixture, _cli, checksum) => write(checksum, `${'g'.repeat(64)}  ${assets[0][1]}\n`),
  },
  {
    name: 'extra checksum field', diagnostic: 'must reference its adjacent operational CLI',
    mutate: (_fixture, _cli, checksum) => write(checksum, readFileSync(checksum, 'utf8').trimEnd() + ' extra\n'),
  },
  {
    name: 'multiple checksum records', diagnostic: 'must contain exactly one record',
    mutate: (_fixture, _cli, checksum) => write(checksum, readFileSync(checksum, 'utf8').repeat(2)),
  },
  {
    name: 'extra regular artifact', diagnostic: 'Unexpected release artifact entries', allHashes: true,
    mutate: (fixture) => write(join(fixture.artifactRoot, 'release-disksage-ubuntu-22.04-1', 'extra.txt'), 'not admitted'),
  },
  {
    name: 'missing checksum', diagnostic: 'Expected exactly one release artifact named', noHashes: true,
    mutate: (_fixture, _cli, checksum) => rmSync(checksum),
  },
  {
    name: 'extra checksum', diagnostic: 'Expected six operational CLI checksum files', noHashes: true,
    mutate: (fixture, _cli, checksum) => write(join(fixture.artifactRoot, 'release-disksage-ubuntu-22.04-1', 'extra.sha256'), readFileSync(checksum)),
  },
  {
    name: 'duplicate CLI', diagnostic: 'Unexpected release artifact entries',
    mutate: (fixture, cli) => write(join(fixture.artifactRoot, 'release-disksage-windows-2022-1', assets[0][1]), readFileSync(cli)),
  },
  {
    name: 'nonregular artifact', diagnostic: 'non-regular path',
    mutate: (fixture, cli) => symlinkSync(cli, join(fixture.artifactRoot, 'release-disksage-ubuntu-22.04-1', 'alias')),
  },
];

for (const program of ['standalone', 'workflow'] as const) {
  // These read-only admission contracts stay active on every platform.
  describe(`${program} checksum path expansion source contract`, () => {
    it('retains quoted path expansion and real checksum admission without external path helpers', () => {
      const source = program === 'workflow'
        ? workflowProgram()
        : readFileSync(join(repositoryRoot, '.github/scripts/verify-release-artifacts.sh'), 'utf8');
      expect(source).toContain('checksum_dir="${checksum_file%/*}"');
      expect(source).toContain('checksum_name="${checksum_file##*/}"');
      expect(source).not.toMatch(/\$\((?:dirname|basename)\s/);
      expect(source).toContain('if [[ ${#checksum_files[@]} -ne 6 ]]; then');
      expect(source).toContain('if [[ $line_count -ne 1 ]]; then');
      expect(source).toContain('[[ "$recorded_name" != "$expected_asset_name" ]]');
      expect(source).toContain('[[ -n "$extra_field" ]]');
      expect(source).toContain('(cd "$checksum_dir" && sha256sum --check "$checksum_name")');
      expect(source).toContain('(cd "$checksum_dir" && shasum -a 256 --check "$checksum_name")');
    });
  });

  // Only native POSIX execution/fixtures are skipped; the contracts above are not.
  describe.skipIf(process.platform === 'win32')(`${program} checksum path expansion`, () => {
    for (const backend of ['sha256sum', 'shasum'] as const) {
      for (const [shellName, shell] of [['PATH Bash', real?.bash ?? 'bash'], ['system Bash', '/bin/bash']] as const) {
        it(`accepts spaces/Unicode/leading-dash roots with ${backend} and ${shellName} without external path helpers`, () => {
          withFixture(program, backend, (fixture) => {
            const result = run(program, fixture, shell);
            expect.soft(result.error).toBeUndefined();
            expect.soft(result.status, result.stderr).toBe(0);
            expect.soft(result.stderr).toBe('');
            expect.soft(result.stdout.match(/: OK\n/g)).toHaveLength(6);
            assertSuccessfulHashes(fixture, backend);
            expect(readFileSync(fixture.helperLog, 'utf8')).toBe('');
          });
        });
      }
      for (const negative of negatives) {
        it(`rejects ${negative.name} with real ${backend}`, () => {
          withFixture(program, backend, (fixture) => {
            const cli = join(fixture.artifactRoot, 'release-disksage-ubuntu-22.04-1', assets[0][1]);
            negative.mutate(fixture, cli, `${cli}.sha256`);
            const result = run(program, fixture, '/bin/bash');
            expect(result.error).toBeUndefined();
            expect(result.status).not.toBeNull();
            expect(result.status).not.toBe(0);
            const diagnostic = program === 'workflow' && negative.name === 'duplicate CLI'
              ? `Expected exactly one release artifact named ${assets[0][1]}, found 2.`
              : negative.diagnostic;
            expect(result.stdout + result.stderr).toContain(diagnostic);
            expect(readFileSync(fixture.helperLog, 'utf8')).toBe('');
            const rows = shaRows(fixture);
            if (negative.realHashFailure) {
              expect(rows.some(([tool, status, cwd, args]) => {
                if (tool !== backend || status !== '1') return false;
                const observedPath = join(cwd, args.split(' ').at(-1)!);
                return realpathSync(observedPath) === realpathSync(`${cli}.sha256`);
              }), JSON.stringify({ rows, checksum: `${cli}.sha256` })).toBe(true);
            } else {
              expect(rows.every(([tool, status]) => tool === backend && status === '0')).toBe(true);
              if (negative.allHashes) assertSuccessfulHashes(fixture, backend);
              if (negative.noHashes) expect(rows).toHaveLength(0);
              if (['wrong adjacent owner', 'decoy ownership', 'malformed digest', 'extra checksum field', 'multiple checksum records'].includes(negative.name)) {
                expect(rows.some(([, , cwd, args]) => join(cwd, args.split(' ').at(-1)!) === `${cli}.sha256`)).toBe(false);
              }
            }
          });
        });
      }
    }
    if (program === 'workflow') {
      for (const backend of ['sha256sum', 'shasum'] as const) {
        it(`retains actual SBOM byte validation with ${backend}`, () => {
          withFixture(program, backend, (fixture) => {
            write(join(fixture.artifactRoot, 'sbom/disksage.spdx.json'), JSON.stringify({ spdxVersion: 'invalid' }));
            const result = run(program, fixture, '/bin/bash');
            expect(result.error).toBeUndefined();
            expect(result.status).toBe(2);
            expect(result.stderr).toContain('error:spdx-document-header-invalid');
            assertSuccessfulHashes(fixture, backend);
            expect(readFileSync(fixture.helperLog, 'utf8')).toBe('');
          });
        });
      }
    }
  });
}
