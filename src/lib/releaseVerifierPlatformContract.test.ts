import { readFileSync, realpathSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { transpileModule, ScriptTarget, ModuleKind } from 'typescript';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// Compile the exact source before the bounded child launch, avoiding a second cold
// TypeScript compiler initialization inside the collection deadline. The fresh child
// verifies source identity and retains its original file URL/import.meta.dirname.
// A Node 20 module.register hook adapts only this module's load and Vitest registration;
// source-contract callbacks use the installed Vitest Chai matchers.
// This is a win32 collection simulation, not native Windows shell execution.
const windowsImportProbe = String.raw`
  import assert from 'node:assert/strict';
  import childProcess from 'node:child_process';
  import fs from 'node:fs';
  import fsPromises from 'node:fs/promises';
  import { register, syncBuiltinESMExports } from 'node:module';
  import { pathToFileURL } from 'node:url';
  import { chai, JestChaiExpect } from '@vitest/expect';

  chai.use(JestChaiExpect);
  const cases = [];
  let skipped = false;
  const describe = (_name, callback) => callback();
  describe.skipIf = (condition) => (_name, callback) => {
    const previous = skipped;
    skipped ||= condition;
    try { callback(); } finally { skipped = previous; }
  };
  const it = (name, callback) => cases.push({ name, callback, skipped });
  globalThis.__releasePlatformCollector = { describe, it, expect: chai.expect };
  const registrationURL = 'data:text/javascript,' + encodeURIComponent(
    'export const { describe, it, expect } = globalThis.__releasePlatformCollector;'
  );
  const packet = JSON.parse(fs.readFileSync(0, 'utf8'));
  const targetFilename = fs.realpathSync(process.argv[1]);
  const targetURL = pathToFileURL(targetFilename).href;
  assert.equal(packet.targetURL, targetURL, 'Exact target URL must match the compiled source');
  assert.equal(packet.original, fs.readFileSync(targetFilename, 'utf8'), 'Exact source bytes must match before target import');
  assert.equal(typeof packet.compiled, 'string');
  assert.notEqual(packet.compiled, '', 'Compiled target must be nonempty');
  // register runs trusted hooks in a separate loader thread. Transport only the
  // already-compiled exact target; never read or execute fixture code there.
  const hookURL = 'data:text/javascript,' + encodeURIComponent([
    'let targetURL, registrationURL, source;',
    'export function initialize(data) { ({ targetURL, registrationURL, source } = data); }',
    'export function resolve(specifier, context, nextResolve) {',
    '  if (specifier === "vitest" && context.parentURL === targetURL)',
    '    return { url: registrationURL, shortCircuit: true };',
    '  return nextResolve(specifier, context);',
    '}',
    'export function load(url, context, nextLoad) {',
    '  if (url === targetURL) return { format: "module", shortCircuit: true, source };',
    '  return nextLoad(url, context);',
    '}',
  ].join('\n'));
  register(hookURL, {
    parentURL: targetURL, data: { targetURL, registrationURL, source: packet.compiled },
  });

  // Block launch/fixture side effects inside this private child only. Never
  // replace real installed tools or modify the parent process platform.
  const forbidden = [];
  const deny = (name) => (...args) => {
    forbidden.push(name + (name === 'spawnSync' ? ':' + args[0] : ''));
    throw new Error('Launch/fixture side effect forbidden during win32 collection: ' + name);
  };
  for (const name of ['spawn', 'spawnSync', 'exec', 'execSync', 'execFile', 'execFileSync', 'fork']) {
    childProcess[name] = deny(name);
  }
  const mutations = ['appendFile', 'chmod', 'chown', 'copyFile', 'cp', 'fchmod', 'fchown',
    'ftruncate', 'futimes', 'lchmod', 'lchown', 'link', 'lutimes', 'mkdir', 'mkdtemp',
    'rename', 'rm', 'rmdir', 'symlink', 'truncate', 'unlink', 'utimes', 'write',
    'writeFile', 'writev'];
  for (const name of mutations) {
    for (const variant of [name, name + 'Sync']) {
      if (typeof fs[variant] === 'function') fs[variant] = deny(variant);
    }
    if (typeof fsPromises[name] === 'function') fsPromises[name] = deny('promises.' + name);
  }
  fs.createWriteStream = deny('createWriteStream');
  // Read-only opens remain usable for universal source contracts. Deny writable
  // descriptors too, rather than guarding only the higher-level fixture helpers.
  const writable = fs.constants.O_WRONLY | fs.constants.O_RDWR | fs.constants.O_CREAT
    | fs.constants.O_TRUNC | fs.constants.O_APPEND;
  for (const [owner, name] of [[fs, 'open'], [fs, 'openSync'], [fsPromises, 'open']]) {
    const original = owner[name];
    owner[name] = (path, flags, ...args) => {
      if (typeof flags === 'string' ? flags !== 'r' && flags !== 'rs'
        : typeof flags !== 'number' || (flags & writable) !== 0) {
        return deny(name)(path);
      }
      return original.call(owner, path, flags, ...args);
    };
  }
  syncBuiltinESMExports();
  Object.defineProperty(process, 'platform', { value: 'win32' });

  let importError;
  try { await import(targetURL); } catch (error) { importError = error.stack; }
  const active = cases.filter((test) => !test.skipped);
  const skippedCases = cases.filter((test) => test.skipped);
  const failures = [];
  for (const test of active) {
    try { await test.callback(); } catch (error) { failures.push({ name: test.name, error: error.stack }); }
  }
  console.log(JSON.stringify({ platform: process.platform, importError, forbidden,
    active: active.map((test) => test.name), skipped: skippedCases.map((test) => test.name), failures }));
  assert.equal(importError, undefined, 'Actual test module must import on win32 without Unix lookup');
  assert.deepEqual(forbidden, [], 'Windows collection and universal contracts must have no launch/fixture side effects');
  assert.equal(skippedCases.length, 54, 'Retain all 54 Unix executable cases as platform-scoped coverage');
  assert.equal(active.length, 2, 'Both actual admission source contracts must execute on Windows');
  assert.deepEqual(failures, [], 'Platform-independent source contracts must pass on Windows');
`;

function compiledPacket() {
  const targetFilename = realpathSync(resolve(import.meta.dirname, 'releaseVerifierPathExpansion.test.ts'));
  const original = readFileSync(targetFilename, 'utf8');
  const compiled = transpileModule(original, {
    fileName: targetFilename, reportDiagnostics: true,
    compilerOptions: { target: ScriptTarget.ES2022, module: ModuleKind.ESNext },
  });
  expect(compiled.diagnostics ?? []).toEqual([]);
  return { targetURL: pathToFileURL(targetFilename).href, original, compiled: compiled.outputText };
}

function runCollection(packet: ReturnType<typeof compiledPacket>) {
  return spawnSync(process.execPath, ['--input-type=module', '-e', windowsImportProbe,
    realpathSync(resolve(import.meta.dirname, 'releaseVerifierPathExpansion.test.ts'))], {
    input: JSON.stringify(packet),
    cwd: resolve(import.meta.dirname, '../..'), encoding: 'utf8', timeout: 5000,
    maxBuffer: 1024 * 1024,
  });
}

describe('release verifier platform collection contract', () => {
  it('imports the actual module as win32 without Unix tools or fixture setup while executing source contracts', () => {
    const result = runCollection(compiledPacket());
    expect(result.error).toBeUndefined();
    expect(result.status, result.stdout + result.stderr).toBe(0);
    const receipt = JSON.parse(result.stdout.trim());
    expect(receipt.platform).toBe('win32');
    expect(receipt.forbidden).toEqual([]);
    expect(receipt.active).toHaveLength(2);
    expect(receipt.skipped).toHaveLength(54);
    expect(receipt.failures).toEqual([]);
  });

  it.each(['targetURL', 'original'] as const)('rejects mismatched %s before target execution', (field) => {
    const packet = compiledPacket();
    packet[field] += 'source-drift';
    const result = runCollection(packet);
    expect(result.error).toBeUndefined();
    expect(result.status).toBe(1);
    expect(result.stdout).toBe('');
    expect(result.stderr).toContain(field === 'targetURL' ? 'Exact target URL' : 'Exact source bytes');
  });
});
