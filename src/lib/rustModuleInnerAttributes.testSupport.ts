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

function readInnerAttribute(source: string, start: number): [string, number] | null {
  let bracketDepth = 0;

  for (let cursor = start + 2; cursor < source.length; cursor += 1) {
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

export function leadingRustModuleInnerAttributes(source: string): string[] {
  const attributes: string[] = [];
  let cursor = source.charCodeAt(0) === 0xfeff ? 1 : 0;

  while (cursor < source.length) {
    while (cursor < source.length && /\s/.test(source[cursor])) {
      cursor += 1;
    }

    if (source.startsWith('//', cursor)) {
      const newline = source.indexOf('\n', cursor + 2);
      cursor = newline === -1 ? source.length : newline + 1;
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

export function missingDocsLintLevels(source: string): Set<RustLintLevel> {
  const levels = new Set<RustLintLevel>();

  for (const attribute of leadingRustModuleInnerAttributes(source)) {
    const match = attribute.match(
      /^#!\[\s*(allow|warn|deny|forbid)\s*\(([\s\S]*)\)\s*\]$/
    );
    if (match !== null && /\bmissing_docs\b/.test(match[2])) {
      levels.add(match[1] as RustLintLevel);
    }
  }

  return levels;
}
