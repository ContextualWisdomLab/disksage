import { execFileSync } from 'node:child_process';
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const workflow = readFileSync(
  new URL('../../.github/workflows/test.yml', import.meta.url),
  'utf8',
);
const diagnosticHelper = fileURLToPath(
  new URL('../../.github/scripts/bound-coverage-command-diagnostic.sh', import.meta.url),
);

describe('bounded Rust coverage command diagnostic', () => {
  it('preserves a compiler diagnostic after an oversized rustc invocation', () => {
    const directory = mkdtempSync(join(tmpdir(), 'disksage-coverage-diagnostic-'));
    try {
      const rawLog = join(directory, 'raw.log');
      const boundedLog = join(directory, 'bounded.log');
      const compilerDiagnostic =
        'error[E0308]: mismatched types\n --> src-tauri/tests/example.rs:41:7\n';
      writeFileSync(
        rawLog,
        `${'rustc --crate-name disksage '.padEnd(50_000, 'x')}\n${compilerDiagnostic}`,
      );

      execFileSync('bash', [diagnosticHelper, rawLog, boundedLog]);

      const diagnostic = readFileSync(boundedLog, 'utf8');
      expect(diagnostic).toContain(' ... [line truncated]');
      expect(diagnostic).toContain(compilerDiagnostic);
      expect(Buffer.byteLength(diagnostic)).toBeLessThan(5_000);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it('wires the exact coverage step through the executable helper', () => {
    expect(workflow).toContain(
      'bash .github/scripts/bound-coverage-command-diagnostic.sh coverage-command.raw.log coverage-command.bounded.log',
    );
    expect(workflow).toContain(
      'rm -f coverage-command.raw.log coverage-command.bounded.log',
    );
  });
});
