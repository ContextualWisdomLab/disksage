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

  it('preserves compiler diagnostics that fall between long bounded log edges', () => {
    const directory = mkdtempSync(join(tmpdir(), 'disksage-coverage-diagnostic-middle-'));
    try {
      const rawLog = join(directory, 'raw.log');
      const boundedLog = join(directory, 'bounded.log');
      const compilerDiagnostic =
        'error[E0753]: expected outer doc comment\n --> src-tauri/src/bin/example.rs:1:1\n';
      const prefixNoise = Array.from(
        { length: 500 },
        (_, index) => `prefix-noise-${String(index).padStart(4, '0')} ${'x'.repeat(80)}`,
      ).join('\n');
      const suffixNoise = Array.from(
        { length: 500 },
        (_, index) => `suffix-noise-${String(index).padStart(4, '0')} ${'y'.repeat(80)}`,
      ).join('\n');
      writeFileSync(
        rawLog,
        `${prefixNoise}\n${compilerDiagnostic}${suffixNoise}\n`,
      );

      execFileSync('bash', [diagnosticHelper, rawLog, boundedLog]);

      const diagnostic = readFileSync(boundedLog, 'utf8');
      expect(diagnostic).toContain(compilerDiagnostic);
      expect(Buffer.byteLength(diagnostic)).toBeLessThanOrEqual(32_768);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it('prioritizes compiler errors when earlier warnings exhaust the focus budget', () => {
    const directory = mkdtempSync(join(tmpdir(), 'disksage-coverage-diagnostic-warnings-'));
    try {
      const rawLog = join(directory, 'raw.log');
      const boundedLog = join(directory, 'bounded.log');
      const compilerDiagnostic =
        'error[E0425]: cannot find value `missing` in this scope\n --> src-tauri/src/lib.rs:777:9\n';
      const prefixNoise = Array.from(
        { length: 180 },
        (_, index) => `prefix-noise-${String(index).padStart(4, '0')} ${'x'.repeat(80)}`,
      ).join('\n');
      const warnings = Array.from(
        { length: 220 },
        (_, index) =>
          `warning: pre-error warning ${String(index).padStart(4, '0')} ${'w'.repeat(70)}\n` +
          ` --> src-tauri/src/warn${String(index).padStart(4, '0')}.rs:1:1`,
      ).join('\n');
      const suffixNoise = Array.from(
        { length: 500 },
        (_, index) => `suffix-noise-${String(index).padStart(4, '0')} ${'y'.repeat(80)}`,
      ).join('\n');
      writeFileSync(
        rawLog,
        `${prefixNoise}\n${warnings}\n${compilerDiagnostic}${suffixNoise}\n`,
      );

      execFileSync('bash', [diagnosticHelper, rawLog, boundedLog]);

      const diagnostic = readFileSync(boundedLog, 'utf8');
      expect(diagnostic).toContain(compilerDiagnostic);
      expect(Buffer.byteLength(diagnostic)).toBeLessThanOrEqual(32_768);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it('preserves ANSI-colored Rust errors emitted by the coverage runner', () => {
    const directory = mkdtempSync(join(tmpdir(), 'disksage-coverage-diagnostic-ansi-'));
    try {
      const rawLog = join(directory, 'raw.log');
      const boundedLog = join(directory, 'bounded.log');
      const compilerDiagnostic =
        '\u001b[31merror[E0425]\u001b[0m: cannot find value `missing` in this scope\n' +
        '\u001b[34m --> \u001b[0msrc-tauri/src/lib.rs:777:9\n';
      const prefixNoise = Array.from(
        { length: 500 },
        (_, index) => `prefix-noise-${String(index).padStart(4, '0')} ${'x'.repeat(80)}`,
      ).join('\n');
      const suffixNoise = Array.from(
        { length: 500 },
        (_, index) => `suffix-noise-${String(index).padStart(4, '0')} ${'y'.repeat(80)}`,
      ).join('\n');
      writeFileSync(
        rawLog,
        `${prefixNoise}\n${compilerDiagnostic}${suffixNoise}\n`,
      );

      execFileSync('bash', [diagnosticHelper, rawLog, boundedLog]);

      const diagnostic = readFileSync(boundedLog, 'utf8');
      expect(diagnostic).toContain('error[E0425]');
      expect(diagnostic).toContain('src-tauri/src/lib.rs:777:9');
      expect(Buffer.byteLength(diagnostic)).toBeLessThanOrEqual(32_768);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });

  it('preserves Rust test panic context from the middle of an oversized log', () => {
    const directory = mkdtempSync(join(tmpdir(), 'disksage-coverage-diagnostic-panic-'));
    try {
      const rawLog = join(directory, 'raw.log');
      const boundedLog = join(directory, 'bounded.log');
      const panicDiagnostic =
        "thread 'provider_oauth::tests::oauth_connection_document_bounds_and_links_fail_closed' panicked at src-tauri/src/provider_oauth.rs:1401:9:\n" +
        'assertion `left == right` failed\n' +
        '  left: "oauth-connection-document-permissions-unsafe"\n' +
        ' right: "oauth-connection-document-too-large"\n' +
        'note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n';
      const prefixNoise = Array.from(
        { length: 500 },
        (_, index) => `prefix-noise-${String(index).padStart(4, '0')} ${'x'.repeat(80)}`,
      ).join('\n');
      const suffixNoise = Array.from(
        { length: 500 },
        (_, index) => `suffix-noise-${String(index).padStart(4, '0')} ${'y'.repeat(80)}`,
      ).join('\n');
      writeFileSync(rawLog, `${prefixNoise}\n${panicDiagnostic}${suffixNoise}\n`);

      execFileSync('bash', [diagnosticHelper, rawLog, boundedLog]);

      const diagnostic = readFileSync(boundedLog, 'utf8');
      expect(diagnostic).toContain("thread 'provider_oauth::tests::oauth_connection_document_bounds_and_links_fail_closed' panicked at");
      expect(diagnostic).toContain('assertion `left == right` failed');
      expect(diagnostic).toContain('left: "oauth-connection-document-permissions-unsafe"');
      expect(diagnostic).toContain('right: "oauth-connection-document-too-large"');
      expect(Buffer.byteLength(diagnostic)).toBeLessThanOrEqual(32_768);
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
