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

function functionDeclaration(name: string): string {
  const signature = `fn ${name}(`;
  const start = safetySource.indexOf(signature);
  expect(start, `${name} must exist`).toBeGreaterThanOrEqual(0);
  const bodyStart = safetySource.indexOf('{', start);
  expect(bodyStart).toBeGreaterThan(start);
  return safetySource.slice(start, bodyStart);
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
  it('fails closed when a supplied catalog root does not authorize the target', () => {
    const body = withoutRustComments(functionBody('trash_delete_if_identity_with_catalog_root'));
    const failedCatalogAuthority = body.indexOf(
      'if catalog_root.is_some() && !catalog_authorized',
    );
    const targetIdentityRead = body.indexOf('filesystem_object_id(path)');

    expect(failedCatalogAuthority).toBeGreaterThanOrEqual(0);
    expect(targetIdentityRead).toBeGreaterThan(failedCatalogAuthority);

    const gate = body.slice(failedCatalogAuthority, targetIdentityRead);
    expect(gate).toContain('return Err(SafetyError::Protected(path.to_path_buf()))');
  });

  it('captures the initially authorized catalog-root object identity before staging', () => {
    const body = withoutRustComments(functionBody('trash_delete_if_identity_with_catalog_root'));
    const rootIdentityCapture = body.indexOf('filesystem_object_id(root)');
    const stagingDirectory = body.indexOf('let staging_dir =');

    expect(rootIdentityCapture).toBeGreaterThanOrEqual(0);
    expect(stagingDirectory).toBeGreaterThan(rootIdentityCapture);
  });

  it('revalidates reviewed root and target identities immediately before the staging rename', () => {
    const body = withoutRustComments(functionBody('trash_delete_if_identity_with_catalog_root'));
    const stagingBoundary = body.indexOf('let result =');
    const rename = body.indexOf('std::fs::rename(path, &staged)', stagingBoundary);

    expect(stagingBoundary).toBeGreaterThanOrEqual(0);
    expect(rename).toBeGreaterThan(stagingBoundary);

    const beforeRename = body.slice(stagingBoundary, rename);
    expect(beforeRename).toMatch(
      /revalidate_catalog_root_before_staging\(\s*path,\s*root,\s*expected_catalog_root_id,\s*expected_object_id\s*\)\?;?/,
    );
  });

  it('requires the helper to fail closed on root replacement, parent drift, protection, and target replacement', () => {
    const declaration = withoutRustComments(functionDeclaration('revalidate_catalog_root_before_staging'));
    expect(declaration).toMatch(/path\s*:\s*&Path/);
    expect(declaration).toMatch(/root\s*:\s*&Path/);
    expect(declaration).toMatch(/expected_catalog_root_id\s*:\s*&str/);
    expect(declaration).toMatch(/expected_object_id\s*:\s*&str/);

    const helper = withoutRustComments(functionBody('revalidate_catalog_root_before_staging'));
    expect(helper).toContain('std::fs::symlink_metadata(root)');
    expect(helper).toContain('metadata.is_dir()');
    expect(helper).toContain('metadata.file_type().is_symlink()');
    expect(helper).toContain('is_windows_reparse_point(&metadata)');
    expect(helper).toContain('filesystem_object_id(root)');
    expect(helper).toContain('expected_catalog_root_id');
    expect(helper).toContain('std::fs::canonicalize(root)');
    expect(helper).toContain('std::fs::canonicalize(path)');
    expect(helper).toContain('parent()');
    expect(helper).toContain('is_explicitly_protected');
    expect(helper).toContain('filesystem_object_id(path)');
    expect(helper).toContain('expected_object_id');
  });

  it('rejects every Windows reparse-point catalog root rather than only symbolic links', () => {
    const windowsHelper = withoutRustComments(functionBody('is_windows_reparse_point'));
    expect(windowsHelper).toContain('MetadataExt');
    expect(windowsHelper).toContain('file_attributes()');
    expect(windowsHelper).toContain('FILE_ATTRIBUTE_REPARSE_POINT');
    expect(windowsHelper).toMatch(/&\s*FILE_ATTRIBUTE_REPARSE_POINT\s*!=\s*0/);

    const authorization = withoutRustComments(
      functionBody('trash_delete_if_identity_with_catalog_root'),
    );
    expect(authorization).toContain('!is_windows_reparse_point(&metadata)');
  });

  it('does not accept a commented revalidation call as executable authority', () => {
    const fixture = `
      let result = (|| -> Result<(), SafetyError> {
        // revalidate_catalog_root_before_staging(path, root, expected_catalog_root_id, expected_object_id)?;
        /* revalidate_catalog_root_before_staging(path, root, expected_catalog_root_id, expected_object_id)?; */
        std::fs::rename(path, &staged)?;
        Ok(())
      })();
    `;

    const executable = withoutRustComments(fixture);
    expect(executable).not.toContain(
      'revalidate_catalog_root_before_staging(path, root, expected_catalog_root_id, expected_object_id)?',
    );
  });
});
