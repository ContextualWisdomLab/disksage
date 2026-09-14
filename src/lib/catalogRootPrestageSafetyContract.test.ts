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

describe('catalog-root Trash staging safety contract', () => {
  it('revalidates catalog-root authority immediately before the staging rename', () => {
    const body = functionBody('trash_delete_if_identity_with_catalog_root');
    const stagingBoundary = body.indexOf('let result =');
    const rename = body.indexOf('std::fs::rename(path, &staged)', stagingBoundary);

    expect(stagingBoundary).toBeGreaterThanOrEqual(0);
    expect(rename).toBeGreaterThan(stagingBoundary);

    const beforeRename = body.slice(stagingBoundary, rename);
    expect(beforeRename).toContain('revalidate_catalog_root_before_staging(path, root)?');
  });
});
