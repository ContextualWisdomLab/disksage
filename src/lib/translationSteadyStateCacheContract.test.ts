import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const bridgeSource = readFileSync('src-tauri/src/translation_resource_bridge.rs', 'utf8');
const libSource = readFileSync('src-tauri/src/lib.rs', 'utf8');

function rustFunctionBody(source: string, name: string): string {
  const signature = `fn ${name}(`;
  const start = source.indexOf(signature);
  expect(start, `${name} must exist`).toBeGreaterThanOrEqual(0);
  const bodyStart = source.indexOf('{', start);
  expect(bodyStart).toBeGreaterThan(start);

  let depth = 0;
  let stringLiteral = false;
  let charLiteral = false;
  let lineComment = false;
  let blockDepth = 0;
  let escaped = false;

  for (let index = bodyStart; index < source.length; index += 1) {
    const current = source[index];
    const next = source[index + 1];

    if (lineComment) {
      if (current === '\n') lineComment = false;
      continue;
    }
    if (blockDepth > 0) {
      if (current === '/' && next === '*') {
        blockDepth += 1;
        index += 1;
      } else if (current === '*' && next === '/') {
        blockDepth -= 1;
        index += 1;
      }
      continue;
    }
    if (stringLiteral || charLiteral) {
      if (escaped) {
        escaped = false;
      } else if (current === '\\') {
        escaped = true;
      } else if (stringLiteral && current === '"') {
        stringLiteral = false;
      } else if (charLiteral && current === "'") {
        charLiteral = false;
      }
      continue;
    }
    if (current === '/' && next === '/') {
      lineComment = true;
      index += 1;
      continue;
    }
    if (current === '/' && next === '*') {
      blockDepth = 1;
      index += 1;
      continue;
    }
    if (current === '"') {
      stringLiteral = true;
      continue;
    }
    if (current === "'") {
      charLiteral = true;
      continue;
    }
    if (current === '{') depth += 1;
    if (current === '}') {
      depth -= 1;
      if (depth === 0) return source.slice(bodyStart + 1, index);
    }
  }

  throw new Error(`${name} body is not balanced`);
}

describe('native translation steady-state cache contract', () => {
  it('moves immutable resource admission and SQLite installation out of every IPC lookup', () => {
    const lookup = rustFunctionBody(bridgeSource, 'get_translation_message');
    expect(lookup).not.toContain('load_current_translation_resource_file');
    expect(lookup).not.toContain('install_current_translation_resource');

    const initialization = rustFunctionBody(bridgeSource, 'initialize_translation_ledger');
    expect(initialization).toContain('load_current_translation_resource_file');
    expect(initialization).toContain('install_current_translation_resource');
  });

  it('binds steady-state cache identity to resource version, locale, and stable screen key', () => {
    expect(bridgeSource).toContain('struct TranslationMessageCacheKey');
    expect(bridgeSource).toMatch(/resource_version\s*:\s*String/);
    expect(bridgeSource).toMatch(/locale\s*:\s*String/);
    expect(bridgeSource).toMatch(/screen_key\s*:\s*String/);
    expect(bridgeSource).toContain('MAX_TRANSLATION_MESSAGE_CACHE_ENTRIES');
  });

  it('populates runtime state once during Tauri setup and serves lookup through managed state', () => {
    expect(libSource).toContain('translation_resource_bridge::initialize_translation_ledger');
    expect(libSource).toContain('.manage(translation_ledger)');

    const lookup = rustFunctionBody(bridgeSource, 'get_translation_message');
    expect(lookup).toContain('TranslationLedgerRuntime');
    expect(lookup).toContain('lookup_translation_message');
  });
});
