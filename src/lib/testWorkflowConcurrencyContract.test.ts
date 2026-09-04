import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

function readTestWorkflow(): string {
  return readFileSync(resolve(repositoryRoot, '.github/workflows/test.yml'), 'utf8').replace(
    /\r\n?/g,
    '\n',
  );
}

describe('Test workflow supersession', () => {
  it('cancels only obsolete first-attempt pull-request work without making reruns self-cancel', () => {
    const workflow = readTestWorkflow();
    const concurrencyStart = workflow.indexOf('concurrency:');
    const permissionsStart = workflow.indexOf('permissions:');

    expect(concurrencyStart).toBeGreaterThanOrEqual(0);
    expect(permissionsStart).toBeGreaterThan(concurrencyStart);

    const concurrencyBlock = workflow.slice(concurrencyStart, permissionsStart);
    expect(concurrencyBlock).toContain(
      'group: ${{ github.workflow }}-${{ github.repository }}-${{ github.event.pull_request.number || github.run_id }}',
    );
    expect(concurrencyBlock).toContain(
      "cancel-in-progress: ${{ github.event_name == 'pull_request' && github.run_attempt == 1 }}",
    );
    expect(concurrencyBlock).not.toContain('github.sha');
    expect(concurrencyBlock).not.toContain('pull_request.head.sha');
  });

  it('keeps every native Test job explicitly time-bounded', () => {
    const workflow = readTestWorkflow();

    expect(workflow).toContain(
      '  test:\n    runs-on: ubuntu-latest\n    timeout-minutes: 60\n',
    );
    expect(workflow).toContain(
      '  windows-home-resolution:\n    runs-on: windows-latest\n    timeout-minutes: 10\n',
    );
    expect(workflow).toContain(
      '  coverage-evidence:\n    runs-on: ubuntu-latest\n    timeout-minutes: 60\n',
    );
    expect(workflow).toContain(
      '  llm-engine-build:\n    runs-on: ubuntu-latest\n    timeout-minutes: 30\n',
    );
  });
});
