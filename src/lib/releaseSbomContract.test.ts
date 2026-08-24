import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const generator = join(repositoryRoot, 'scripts/ci/generate-release-sbom.mjs');

/** Run the source-controlled SBOM generator with deterministic fixture inputs. */
function runGenerator(args: string[], cwd: string) {
  return spawnSync(process.execPath, [generator, ...args], {
    cwd,
    encoding: 'utf8',
  });
}

describe('release SBOM generator', () => {
  it('binds Cargo and npm lock inventories to a deterministic SPDX document', () => {
    const root = mkdtempSync(join(tmpdir(), 'disksage-sbom-contract-'));
    try {
      const cargoPath = join(root, 'cargo.json');
      const npmPath = join(root, 'package-lock.json');
      const outputPath = join(root, 'release-artifacts', 'disksage.spdx.json');
      const cargoRoot = 'path+file:///workspace/disksage#disksage@0.1.0';
      writeFileSync(cargoPath, JSON.stringify({
        packages: [{ id: cargoRoot, name: 'disksage', version: '0.1.0', source: null, license: 'MIT' }],
        resolve: { root: cargoRoot, nodes: [{ id: cargoRoot, dependencies: [] }] },
      }));
      writeFileSync(npmPath, JSON.stringify({
        lockfileVersion: 3,
        packages: {
          '': { name: 'disksage', version: '0.1.0' },
          'node_modules/parent': {
            version: '2.0.0',
            dependencies: { example: '^1.0.0' },
            resolved: 'https://registry.npmjs.org/parent/-/parent-2.0.0.tgz',
            license: 'MIT',
          },
          'node_modules/example': {
            version: '1.2.3',
            resolved: 'https://registry.npmjs.org/example/-/example-1.2.3.tgz',
            license: 'MIT',
          },
        },
      }));

      const generated = runGenerator([
        '--cargo-metadata', cargoPath,
        '--npm-lock', npmPath,
        '--source-revision', 'deadbeef',
        '--created', '2026-01-01T09:00:00+09:00',
        '--output', outputPath,
      ], root);
      expect(generated.status).toBe(0);
      const document = JSON.parse(readFileSync(outputPath, 'utf8')) as {
        spdxVersion: string;
        documentNamespace: string;
        packages: Array<{ SPDXID: string; name: string }>;
        relationships: Array<{
          spdxElementId: string;
          relationshipType: string;
          relatedSpdxElement: string;
        }>;
      };
      expect(document.spdxVersion).toBe('SPDX-2.3');
      expect(document.documentNamespace).toContain('/sbom/deadbeef');
      expect(document.packages.map((pkg) => pkg.name)).toEqual(
        expect.arrayContaining(['cargo:disksage', 'npm:parent', 'npm:example']),
      );
      const parentId = document.packages.find((pkg) => pkg.name === 'npm:parent')?.SPDXID;
      const exampleId = document.packages.find((pkg) => pkg.name === 'npm:example')?.SPDXID;
      expect(document.relationships).toContainEqual({
        spdxElementId: parentId,
        relationshipType: 'DEPENDS_ON',
        relatedSpdxElement: exampleId,
      });

      const validated = runGenerator(['--validate', outputPath], root);
      expect(validated.status).toBe(0);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it('rejects private workstation paths before publication', () => {
    const root = mkdtempSync(join(tmpdir(), 'disksage-sbom-invalid-'));
    try {
      const outputPath = join(root, 'private.json');
      writeFileSync(outputPath, JSON.stringify({
        spdxVersion: 'SPDX-2.3',
        dataLicense: 'CC0-1.0',
        SPDXID: 'SPDXRef-DOCUMENT',
        name: 'disksage-deadbeef',
        documentNamespace: 'https://github.com/ContextualWisdomLab/disksage/sbom/deadbeef',
        creationInfo: { created: '2026-01-01T00:00:00.000Z', creators: ['Tool: disksage-release-sbom'] },
        documentDescribes: ['SPDXRef-Cargo-root'],
        packages: [{
          SPDXID: 'SPDXRef-Cargo-root',
          name: 'cargo:disksage',
          versionInfo: '0.1.0',
          downloadLocation: '/private/tmp/secret',
          filesAnalyzed: false,
          licenseConcluded: 'NOASSERTION',
          licenseDeclared: 'NOASSERTION',
          supplier: 'NOASSERTION',
        }],
        relationships: [],
      }));
      const result = runGenerator(['--validate', outputPath], root);
      expect(result.status).not.toBe(0);
      expect(result.stderr).toContain('spdx-private-path-present');
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
