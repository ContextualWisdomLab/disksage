import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * Read one repository document from the project root.
 *
 * Keeping the path resolution here makes the contract independent of the
 * caller's working directory while still failing clearly when the authoritative
 * document is missing.
 */
function readRepositoryDocument(relativePath: string): string {
  return readFileSync(resolve(process.cwd(), relativePath), 'utf8');
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
      '## References',
    ];

    for (const heading of requiredHeadings) {
      expect(architecture).toContain(heading);
    }

    expect(architecture).toContain('ContextualWisdomLab/.github');
    expect(architecture).toContain('naruon');
    expect(architecture).toContain('contextual-orchestrator');
    expect(architecture).toContain('exact current head SHA');
    expect(architecture).toContain('independent non-author approval');
    expect(architecture).toContain('snake_case');
    expect(architecture).toContain('APA 7th');
  });

  it('keeps buyer-facing claims linked to authoritative repository evidence', () => {
    const architecture = readRepositoryDocument('ARCHITECTURE.md');

    for (const evidencePath of [
      'README.md',
      'SECURITY.md',
      'CHANGELOG.md',
      '.github/workflows/test.yml',
      '.github/workflows/release.yml',
    ]) {
      expect(architecture).toContain(`\`${evidencePath}\``);
    }

    expect(architecture).toContain('No document, review, status, or artifact from an older head');
    expect(architecture).toContain('does not become durable authorization');
  });
});
