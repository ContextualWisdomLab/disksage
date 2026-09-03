import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

function readWorkflow(): string {
  return readFileSync(
    resolve(repositoryRoot, '.github/workflows/provider-oauth-windows.yml'),
    'utf8',
  ).replace(/\r\n?/g, '\n');
}

describe('Windows provider OAuth executable evidence', () => {
  it('runs the real USERPROFILE process regression on a bounded Windows runner', () => {
    const workflow = readWorkflow();

    expect(workflow).toContain('name: Provider OAuth Windows Contract');
    expect(workflow).toContain('runs-on: windows-2022');
    expect(workflow).toContain('timeout-minutes: 30');
    expect(workflow).toContain('permissions:\n  contents: read');
    expect(workflow).toContain('persist-credentials: false');
    expect(workflow).toContain('ref: ${{ github.event.pull_request.head.sha || github.sha }}');
    expect(workflow).toContain('workspaces: src-tauri');
    expect(workflow).toContain('cache-targets: false');
    expect(workflow).toContain(
      'cargo test --manifest-path src-tauri/Cargo.toml --locked --features cloud-cli --test provider_oauth_cli_process',
    );
    expect(workflow).not.toContain('secrets.');
    expect(workflow).not.toContain('contents: write');
  });
});
