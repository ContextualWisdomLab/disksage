import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const safetySource = readFileSync('src-tauri/src/safety.rs', 'utf8');

function functionBody(name: string): string {
  const signature = `fn ${name}(`;
  const start = safetySource.indexOf(signature);
  expect(start, `${name} must exist`).toBeGreaterThanOrEqual(0);

  const bodyStart = safetySource.indexOf('{', start);
  expect(bodyStart).toBeGreaterThan(start);
  let depth = 0;
  for (let index = bodyStart; index < safetySource.length; index += 1) {
    if (safetySource[index] === '{') depth += 1;
    if (safetySource[index] === '}') {
      depth -= 1;
      if (depth === 0) return safetySource.slice(bodyStart + 1, index);
    }
  }
  throw new Error(`${name} body is not balanced`);
}

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

describe('catalog-root Trash staging safety contract', () => {
  it('revalidates catalog-root authority immediately before the staging rename', () => {
    const body = withoutRustComments(functionBody('trash_delete_if_identity_with_catalog_root'));
    const stagingBoundary = body.indexOf('let result =');
    const rename = body.indexOf('std::fs::rename(path, &staged)', stagingBoundary);

    expect(stagingBoundary).toBeGreaterThanOrEqual(0);
    expect(rename).toBeGreaterThan(stagingBoundary);

    const beforeRename = body.slice(stagingBoundary, rename);
    expect(beforeRename).toContain('revalidate_catalog_root_before_staging(path, root)?');
  });

  it('does not accept a commented revalidation call as executable authority', () => {
    const fixture = `
      let result = (|| -> Result<(), SafetyError> {
        // revalidate_catalog_root_before_staging(path, root)?;
        /* revalidate_catalog_root_before_staging(path, root)?; */
        std::fs::rename(path, &staged)?;
        Ok(())
      })();
    `;

    const executable = withoutRustComments(fixture);
    expect(executable).not.toContain('revalidate_catalog_root_before_staging(path, root)?');
  });
});
