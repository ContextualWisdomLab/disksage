import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/** Read one repository document from the source-controlled project root. */
function readRepositoryDocument(relativePath: string): string {
  return readFileSync(resolve(repositoryRoot, relativePath), 'utf8');
}

const requiredDocuments = [
  'docs/PRD.md',
  'docs/TRD.md',
  'ARCHITECTURE.md',
  'docs/adr/README.md',
  'docs/UML.md',
  'docs/DATA_MODEL.md',
  'docs/API_CONTRACT.md',
  'docs/THREAT_MODEL.md',
  'docs/TEST_STRATEGY.md',
  'docs/OPERABILITY.md',
  'docs/ROADMAP.md',
  'docs/RELEASE_AND_ROLLBACK.md',
  'docs/TRACEABILITY.md',
  'docs/DOCUMENTATION_ASSESSMENT.md',
  'docs/README.md',
  'AGENTS.md',
  'CLAUDE.md',
  'SECURITY.md',
] as const;

describe('canonical DiskSage documentation graph', () => {
  it('keeps every required documentation family discoverable', () => {
    for (const documentPath of requiredDocuments) {
      expect(existsSync(resolve(repositoryRoot, documentPath)), documentPath).toBe(true);
    }
  });

  it('keeps product and technical requirements explicit rather than hidden in chat or PR bodies', () => {
    const prd = readRepositoryDocument('docs/PRD.md');
    const trd = readRepositoryDocument('docs/TRD.md');

    for (const marker of [
      '## Users and buyers',
      '## Functional requirements',
      '## Non-functional requirements',
      '## Degraded and offline behavior',
      '## Explicit non-goals',
      '## Acceptance criteria',
    ]) {
      expect(prd).toContain(marker);
    }

    for (const marker of [
      'Rust',
      'Tauri',
      'Svelte',
      '## Evidence classes',
      'no-clobber',
      'exact current source head',
      'live base',
      'writer lease',
      'NVIDIA_NIM_API_KEY',
      'OpenCode',
    ]) {
      expect(trd).toContain(marker);
    }
  });

  it('keeps architecture, diagrams, ERD, release, roadmap, and ADR governance independently inspectable', () => {
    const architecture = readRepositoryDocument('ARCHITECTURE.md');
    const uml = readRepositoryDocument('docs/UML.md');
    const dataModel = readRepositoryDocument('docs/DATA_MODEL.md');
    const roadmap = readRepositoryDocument('docs/ROADMAP.md');
    const release = readRepositoryDocument('docs/RELEASE_AND_ROLLBACK.md');
    const adrIndex = readRepositoryDocument('docs/adr/README.md');

    expect(architecture).toContain('## Trust and authority boundaries');
    expect(architecture).toContain('## Standalone and modular deployment');
    expect(uml).toContain('```mermaid');
    expect(uml).toContain('## Repository merge and release authority flow');
    expect(dataModel).toContain('Conceptual, logical, and persisted status');
    expect(dataModel).toContain('No central application database is claimed');
    expect(dataModel).toContain('erDiagram');
    expect(roadmap).toContain('## Commercial readiness milestones');
    expect(roadmap).toContain('buyer-visible');
    expect(release).toContain('exact integrated protected head');
    expect(release).toContain('SBOM');
    expect(release).toContain('rollback');

    for (let index = 1; index <= 8; index += 1) {
      expect(adrIndex).toContain(`ADR-${String(index).padStart(4, '0')}`);
    }
  });

  it('keeps documentation completeness and traceability machine-visible', () => {
    const assessment = readRepositoryDocument('docs/DOCUMENTATION_ASSESSMENT.md');
    const traceability = readRepositoryDocument('docs/TRACEABILITY.md');

    expect(assessment).toContain('## Coverage matrix');
    expect(assessment).toContain('## Current conclusion');
    expect(traceability).toContain('requirement');
    expect(traceability).toContain('ADR');
    expect(traceability).toContain('test');
    expect(traceability).toContain('evidence');
  });
});