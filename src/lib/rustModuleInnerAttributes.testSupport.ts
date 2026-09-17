export type RustLintLevel = 'allow' | 'warn' | 'deny' | 'forbid';

function skipNestedBlockComment(source: string, start: number): number {
  let cursor = start + 2;
  let depth = 1;

  while (cursor < source.length && depth > 0) {
    if (source.startsWith('/*', cursor)) {
      depth += 1;
      cursor += 2;
      continue;
    }
    if (source.startsWith('*/', cursor)) {
      depth -= 1;
      cursor += 2;
      continue;
    }
    cursor += 1;
  }

  return cursor;
}

function skipLineComment(source: string, start: number): number {
  const newline = source.indexOf('\n', start + 2);
  return newline === -1 ? source.length : newline + 1;
}

function skipRustQuotedLiteral(source: string, start: number): number | null {
  let cursor = start;
  if (source.startsWith('b"', cursor)) {
    cursor += 1;
  }

  const quote = source[cursor];
  if (quote !== '"' && quote !== "'") {
    return null;
  }

  if (quote === "'") {
    const charLiteral = source.slice(cursor).match(/^'(?:\\.|[^\\'\n])'/);
    return charLiteral === null ? null : cursor + charLiteral[0].length;
  }

  cursor += 1;
  while (cursor < source.length) {
    if (source[cursor] === '\\') {
      cursor += 2;
      continue;
    }
    if (source[cursor] === quote) {
      return cursor + 1;
    }
    cursor += 1;
  }

  return source.length;
}

function skipRustRawString(source: string, start: number): number | null {
  let cursor = start;
  if (source.startsWith('br', cursor)) {
    cursor += 2;
  } else if (source[cursor] === 'r') {
    cursor += 1;
  } else {
    return null;
  }

  let hashes = 0;
  while (source[cursor] === '#') {
    hashes += 1;
    cursor += 1;
  }
  if (source[cursor] !== '"') {
    return null;
  }

  const terminator = `"${'#'.repeat(hashes)}`;
  const end = source.indexOf(terminator, cursor + 1);
  return end === -1 ? source.length : end + terminator.length;
}

function skipRustLiteral(source: string, start: number): number | null {
  return skipRustRawString(source, start) ?? skipRustQuotedLiteral(source, start);
}

function readBracketedAttribute(
  source: string,
  start: number,
  openingBracket: number
): [string, number] | null {
  let bracketDepth = 0;

  for (let cursor = openingBracket; cursor < source.length; cursor += 1) {
    if (source.startsWith('//', cursor)) {
      cursor = skipLineComment(source, cursor) - 1;
      continue;
    }
    if (source.startsWith('/*', cursor)) {
      cursor = skipNestedBlockComment(source, cursor) - 1;
      continue;
    }

    const literalEnd = skipRustLiteral(source, cursor);
    if (literalEnd !== null) {
      cursor = literalEnd - 1;
      continue;
    }

    const character = source[cursor];
    if (character === '[') {
      bracketDepth += 1;
      continue;
    }
    if (character === ']') {
      bracketDepth -= 1;
      if (bracketDepth === 0) {
        return [source.slice(start, cursor + 1), cursor + 1];
      }
    }
  }

  return null;
}

function readInnerAttribute(source: string, start: number): [string, number] | null {
  return readBracketedAttribute(source, start, start + 2);
}

function readOuterAttribute(source: string, start: number): [string, number] | null {
  return readBracketedAttribute(source, start, start + 1);
}

export function leadingRustModuleInnerAttributes(source: string): string[] {
  const attributes: string[] = [];
  let cursor = source.charCodeAt(0) === 0xfeff ? 1 : 0;

  while (cursor < source.length) {
    while (cursor < source.length && /\s/.test(source[cursor])) {
      cursor += 1;
    }

    if (source.startsWith('//', cursor)) {
      cursor = skipLineComment(source, cursor);
      continue;
    }

    if (source.startsWith('/*', cursor)) {
      cursor = skipNestedBlockComment(source, cursor);
      continue;
    }

    if (!source.startsWith('#![', cursor)) {
      break;
    }

    const parsed = readInnerAttribute(source, cursor);
    if (parsed === null) {
      break;
    }
    attributes.push(parsed[0]);
    cursor = parsed[1];
  }

  return attributes;
}

function topLevelRustMetaPaths(source: string): string[] {
  const paths: string[] = [];
  let current = '';
  let cursor = 0;
  let parenDepth = 0;
  let bracketDepth = 0;
  let braceDepth = 0;

  const commit = () => {
    const candidate = current.trim();
    if (/^[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*$/.test(candidate)) {
      paths.push(candidate);
    }
    current = '';
  };

  while (cursor < source.length) {
    if (source.startsWith('//', cursor)) {
      cursor = skipLineComment(source, cursor);
      current += ' ';
      continue;
    }

    if (source.startsWith('/*', cursor)) {
      cursor = skipNestedBlockComment(source, cursor);
      current += ' ';
      continue;
    }

    const literalEnd = skipRustLiteral(source, cursor);
    if (literalEnd !== null) {
      current += '""';
      cursor = literalEnd;
      continue;
    }

    const character = source[cursor];
    if (character === '(') {
      parenDepth += 1;
    } else if (character === ')') {
      parenDepth -= 1;
    } else if (character === '[') {
      bracketDepth += 1;
    } else if (character === ']') {
      bracketDepth -= 1;
    } else if (character === '{') {
      braceDepth += 1;
    } else if (character === '}') {
      braceDepth -= 1;
    } else if (
      character === ',' &&
      parenDepth === 0 &&
      bracketDepth === 0 &&
      braceDepth === 0
    ) {
      commit();
      cursor += 1;
      continue;
    }

    current += character;
    cursor += 1;
  }

  commit();
  return paths;
}

export function missingDocsLintLevels(source: string): Set<RustLintLevel> {
  const levels = new Set<RustLintLevel>();

  for (const attribute of leadingRustModuleInnerAttributes(source)) {
    const match = attribute.match(
      /^#!\[\s*(allow|warn|deny|forbid)\s*\(([\s\S]*)\)\s*\]$/
    );
    if (
      match !== null &&
      topLevelRustMetaPaths(match[2]).includes('missing_docs')
    ) {
      levels.add(match[1] as RustLintLevel);
    }
  }

  return levels;
}

function isOuterLineDocComment(source: string, cursor: number): boolean {
  return source.startsWith('///', cursor) && !source.startsWith('////', cursor);
}

function isOuterBlockDocComment(source: string, cursor: number): boolean {
  return source.startsWith('/**', cursor) && !source.startsWith('/***', cursor);
}

export function publicRustModuleHasOuterDoc(
  source: string,
  moduleName: string
): boolean {
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(moduleName)) {
    return false;
  }

  const declaration = new RegExp(
    `^pub\\s+mod\\s+${moduleName}\\s*;`
  );
  let cursor = source.charCodeAt(0) === 0xfeff ? 1 : 0;
  let hasPendingOuterDoc = false;

  while (cursor < source.length) {
    if (/\s/.test(source[cursor])) {
      cursor += 1;
      continue;
    }

    if (isOuterLineDocComment(source, cursor)) {
      hasPendingOuterDoc = true;
      cursor = skipLineComment(source, cursor);
      continue;
    }

    if (isOuterBlockDocComment(source, cursor)) {
      hasPendingOuterDoc = true;
      cursor = skipNestedBlockComment(source, cursor);
      continue;
    }

    if (source.startsWith('//', cursor)) {
      cursor = skipLineComment(source, cursor);
      continue;
    }

    if (source.startsWith('/*', cursor)) {
      cursor = skipNestedBlockComment(source, cursor);
      continue;
    }

    if (source.startsWith('#[', cursor)) {
      const attribute = readOuterAttribute(source, cursor);
      if (attribute !== null) {
        cursor = attribute[1];
        continue;
      }
    }

    const literalEnd = skipRustLiteral(source, cursor);
    if (literalEnd !== null) {
      hasPendingOuterDoc = false;
      cursor = literalEnd;
      continue;
    }

    const match = source.slice(cursor).match(declaration);
    if (match !== null) {
      return hasPendingOuterDoc;
    }

    hasPendingOuterDoc = false;
    const identifier = source.slice(cursor).match(/^[A-Za-z_][A-Za-z0-9_]*/);
    cursor += identifier?.[0].length ?? 1;
  }

  return false;
}
