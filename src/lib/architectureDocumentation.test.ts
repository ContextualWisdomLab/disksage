import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { ssr } from '../routes/+layout';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/**
 * Read one UTF-8 repository file relative to the source-controlled project root.
 *
 * Resolving from this test module rather than the process working directory keeps
 * the contract deterministic when Vitest is launched from an IDE, a parent
 * workspace, or an isolated CI sandbox.
 */
function readRepositoryDocument(relativePath: string): string {
  return readFileSync(resolve(repositoryRoot, relativePath), 'utf8');
}

describe('acquisition-ready architecture documentation', () => {
  it('defines the product, trust, deployment, modularity, and evidence boundaries', () => {
    const architecture = readRepositoryDocument('ARCHITECTURE.md');
    const requiredHeadings = [
      '# DiskSage Architecture',
      '## Product and system context',
      '## Standalone deployment',
      '## Modular MSA integration',
      '## Trust and authority boundaries',
      '## Data and privacy boundaries',
      '## Reliability, migration, and rollback',
      '## Release and acquisition evidence',
      '## Database object naming',
      '## Architecture change control',
      '## References',
      '## Reference verification note',
    ];

    for (const heading of requiredHeadings) {
      expect(architecture).toContain(heading);
    }

    expect(architecture).toContain('ContextualWisdomLab/.github');
    expect(architecture).toContain('naruon');
    expect(architecture).toContain('contextual-orchestrator');
    expect(architecture).toContain('APA 7th');
  });

  it('keeps exact-head, evidence-path, and database-name claims structurally enforceable', () => {
    const architecture = readRepositoryDocument('ARCHITECTURE.md');
    const evidencePaths = [
      'README.md',
      'SECURITY.md',
      'CHANGELOG.md',
      '.github/workflows/test.yml',
      '.github/workflows/release.yml',
    ];

    for (const evidencePath of evidencePaths) {
      expect(architecture).toContain(`\`${evidencePath}\``);
      expect(existsSync(resolve(repositoryRoot, evidencePath))).toBe(true);
    }

    expect(architecture).toMatch(
      /A pull request may merge only on the exact current head SHA[\s\S]{0,500}independent non-author[\s\S]{0,120}approval is satisfied\./,
    );
    expect(architecture).toMatch(
      /No document, review, status, or artifact from an older head[\s\S]{0,260}durable\s+repository authorization\./,
    );
    expect(architecture).toMatch(
      /Database objects must contain at least two descriptive words and use `snake_case` by\s+default\./,
    );
    expect(architecture).toContain('does not become durable authorization');
  });

  it('defines deterministic read-only and mutating authorization expiry contracts', () => {
    const architecture = readRepositoryDocument('ARCHITECTURE.md');

    for (const requiredContract of [
      '#### Read-only operations',
      '#### Mutating operation contracts',
      '`evidence-incomplete`',
      '`approval-expired`',
      '`approval-clock-invalid`',
      '15 minutes',
      'UTC',
      'monotonic',
    ]) {
      expect(architecture).toContain(requiredContract);
    }
  });

  it('separates runtime operation authorization from repository authorization', () => {
    const architecture = readRepositoryDocument('ARCHITECTURE.md');
    const runtimeStatement =
      'Runtime authorization does not use a repository checkout or Git reference as an operator credential.';
    const runtimeStatementIndex = architecture.indexOf(runtimeStatement);
    const operationScopeIndex = architecture.indexOf(
      'operation scope',
      runtimeStatementIndex,
    );
    const repositoryAuthorizationIndex = architecture.indexOf(
      '### Repository authorization',
      operationScopeIndex,
    );
    const exactHeadIndex = architecture.indexOf(
      'exact current head SHA',
      repositoryAuthorizationIndex,
    );

    expect(architecture).not.toContain('`repository_head_sha`');
    expect(runtimeStatementIndex).toBeGreaterThanOrEqual(0);
    expect(operationScopeIndex).toBeGreaterThan(runtimeStatementIndex);
    expect(repositoryAuthorizationIndex).toBeGreaterThan(operationScopeIndex);
    expect(exactHeadIndex).toBeGreaterThan(repositoryAuthorizationIndex);
  });

  it('records standards and exact marker syntax with precise publication evidence', () => {
    const architecture = readRepositoryDocument('ARCHITECTURE.md');
    const tenantAuthority = readRepositoryDocument(
      'docs/architecture/cloud-review-tenant-authority.md',
    );

    expect(architecture).toContain(
      'Supply-chain Levels for Software Artifacts. (2025).',
    );
    expect(tenantAuthority).toContain('Joint Task Force. (2020).');
    expect(tenantAuthority).toContain(
      'National Institute of Standards and Technology. (2025, August 27).',
    );
    expect(tenantAuthority).toContain(
      '`[organization-tenant-authority-confirmed]` followed by exactly one U+0020 ASCII space',
    );
    expect(tenantAuthority).not.toContain(
      '`[organization-tenant-authority-confirmed] `',
    );
  });

  it('binds test and release entry points to the complete TypeScript coverage gate', () => {
    const packageJson = JSON.parse(readRepositoryDocument('package.json')) as {
      scripts: Record<string, string>;
    };
    const testWorkflow = readRepositoryDocument('.github/workflows/test.yml');
    const releaseWorkflow = readRepositoryDocument('.github/workflows/release.yml');
    const coverageConfiguration = readRepositoryDocument('vitest.config.ts');
    const architecture = readRepositoryDocument('ARCHITECTURE.md');

    expect(packageJson.scripts.build).toBe('npm run coverage && vite build');
    expect(testWorkflow).toContain('- run: npm run coverage');
    expect(releaseWorkflow).toContain('npm run tauri -- build');
    expect(coverageConfiguration).toContain('src/lib/**/*.ts');
    expect(coverageConfiguration).toContain('src/routes/**/*.ts');
    expect(coverageConfiguration).toContain('**/*.test.ts');
    expect(architecture).toMatch(/inherits the same\s+`npm run coverage` gate/);
    expect(ssr).toBe(false);
  });

  it('keeps the canonical product documentation graph discoverable and explicit', () => {
    const requiredDocuments = [
      'docs/PRD.md',
      'docs/TRD.md',
      'ARCHITECTURE.md',
      'docs/adr/README.md',
      'docs/UML.md',
      'docs/DATA_MODEL.md',
      'docs/THREAT_MODEL.md',
      'docs/TEST_STRATEGY.md',
      'docs/OPERABILITY.md',
      'docs/TRACEABILITY.md',
      'docs/DOCUMENTATION_ASSESSMENT.md',
      'AGENTS.md',
      'CLAUDE.md',
      'SECURITY.md',
      'CHANGELOG.md',
    ];

    for (const documentPath of requiredDocuments) {
      expect(existsSync(resolve(repositoryRoot, documentPath))).toBe(true);
    }

    const assessment = readRepositoryDocument('docs/DOCUMENTATION_ASSESSMENT.md');
    const adrIndex = readRepositoryDocument('docs/adr/README.md');
    const dataModel = readRepositoryDocument('docs/DATA_MODEL.md');
    const uml = readRepositoryDocument('docs/UML.md');

    expect(assessment).toContain('## Coverage matrix');
    expect(assessment).toContain('PRD');
    expect(assessment).toContain('TRD');
    expect(assessment).toContain('UML');
    expect(assessment).toContain('ERD');
    expect(adrIndex).toContain('ADR-0001');
    expect(adrIndex).toContain('ADR-0005');
    expect(dataModel).toContain('Conceptual, logical, and persisted status');
    expect(dataModel).toContain('No central application database is claimed by this document');
    expect(uml).toContain('```mermaid');
  });
});