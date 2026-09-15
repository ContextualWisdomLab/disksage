import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');

function withoutRustComments(source: string): string {
  let output = '';
  let blockDepth = 0;
  let lineComment = false;
  let stringLiteral = false;
  let charLiteral = false;
  let escaped = false;

  for (let index = 0; index < source.length; index += 1) {
    const current = source[index];
    const next = source[index + 1];
    if (lineComment) {
      if (current === '\n') {
        lineComment = false;
        output += current;
      }
      continue;
    }
    if (blockDepth > 0) {
      if (current === '/' && next === '*') {
        blockDepth += 1;
        index += 1;
      } else if (current === '*' && next === '/') {
        blockDepth -= 1;
        index += 1;
      } else if (current === '\n') {
        output += current;
      }
      continue;
    }
    if (stringLiteral || charLiteral) {
      output += current;
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
    if (current === '"') stringLiteral = true;
    if (current === "'") charLiteral = true;
    output += current;
  }
  return output;
}

function functionBody(name: string): string {
  const source = withoutRustComments(safetySource);
  const signature = `fn ${name}(`;
  const start = source.indexOf(signature);
  expect(start, `${name} must exist`).toBeGreaterThanOrEqual(0);
  const bodyStart = source.indexOf('{', start);
  expect(bodyStart).toBeGreaterThan(start);
  let depth = 0;
  for (let index = bodyStart; index < source.length; index += 1) {
    if (source[index] === '{') depth += 1;
    if (source[index] === '}') {
      depth -= 1;
      if (depth === 0) return source.slice(bodyStart + 1, index);
    }
  }
  throw new Error(`${name} body is not balanced`);
}

describe('catalog-root authorization race regression contract', () => {
  it('places the deterministic test seam after initial object identity and before root authorization reads', () => {
    const body = functionBody('trash_delete_if_identity_with_catalog_root');
    const initialIdentity = body.indexOf(
      'let initial_catalog_root_id = filesystem_object_id(root)',
    );
    const hook = body.indexOf('run_catalog_root_authorization_hook()', initialIdentity);
    const metadata = body.indexOf('std::fs::symlink_metadata(root)', initialIdentity);

    expect(initialIdentity).toBeGreaterThanOrEqual(0);
    expect(hook).toBeGreaterThan(initialIdentity);
    expect(metadata).toBeGreaterThan(hook);
  });

  it('keeps the authorization race seam test-only and one-shot', () => {
    const executable = withoutRustComments(safetySource);
    expect(executable).toContain('thread_local!');
    expect(executable).toContain('CATALOG_ROOT_AUTHORIZATION_HOOK');
    expect(executable).toContain('fn set_catalog_root_authorization_hook');
    expect(executable).toContain('slot.borrow_mut().take()');
  });

  it('requires a real-filesystem root replacement regression with the same reviewed target object', () => {
    const test = functionBody(
      'catalog_root_authorization_rejects_root_replacement_inside_authorization_window',
    );

    expect(test).toContain('std::fs::rename(&hook_victim, &hook_parked_target)');
    expect(test).toContain('std::fs::rename(&hook_root, &hook_reviewed_root)');
    expect(test).toContain('std::fs::create_dir(&hook_root)');
    expect(test).toContain('std::fs::rename(&hook_parked_target, &hook_victim)');
    expect(test).toContain('trash_delete_if_identity_in_catalog_root');
    expect(test).toContain('Err(SafetyError::Protected(_))');
    expect(test).toContain('filesystem_object_id(&victim).unwrap(), expected_target_id');
    expect(test).toContain('journal_recent(&journal, 10).is_empty()');
    expect(test).toContain('.disksage-trash-');
  });
});
