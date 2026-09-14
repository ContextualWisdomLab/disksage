import { describe, expect, it } from 'vitest';
import {
  leadingRustModuleInnerAttributes,
  missingDocsLintLevels
} from './rustModuleInnerAttributes.testSupport';

describe('Rust module inner-attribute inspection', () => {
  it('ignores comment text and inspects only leading module inner attributes', () => {
    const source = `//! #![allow(missing_docs)]
/*
#![allow(missing_docs)]
*/
#![deny(dead_code, missing_docs)]
use std::path::Path;
// #![allow(missing_docs)]
`;

    expect(leadingRustModuleInnerAttributes(source)).toEqual([
      '#![deny(dead_code, missing_docs)]'
    ]);
    expect([...missingDocsLintLevels(source)]).toEqual(['deny']);
  });

  it('detects missing_docs in allow lists regardless of ordering or line breaks', () => {
    const first = `#![allow(dead_code, missing_docs)]\nuse std::path::Path;\n`;
    const second = `#![allow(\n  missing_docs,\n  dead_code\n)]\nuse std::path::Path;\n`;

    expect(missingDocsLintLevels(first).has('allow')).toBe(true);
    expect(missingDocsLintLevels(second).has('allow')).toBe(true);
  });
});
