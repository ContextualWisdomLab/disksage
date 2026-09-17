import { describe, expect, it } from 'vitest';
import {
  leadingRustModuleInnerAttributes,
  missingDocsLintLevels,
  publicRustModuleHasOuterDoc
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

  it('does not treat comments or lint reasons as missing_docs lint paths', () => {
    const commentOnly = `#![deny(dead_code /* missing_docs */)]\nuse std::path::Path;\n`;
    const reasonOnly = `#![deny(dead_code, reason = "missing_docs")]\nuse std::path::Path;\n`;
    const realLint = `#![deny(dead_code, missing_docs, reason = "documentation contract")]\nuse std::path::Path;\n`;

    expect(missingDocsLintLevels(commentOnly).has('deny')).toBe(false);
    expect(missingDocsLintLevels(reasonOnly).has('deny')).toBe(false);
    expect(missingDocsLintLevels(realLint).has('deny')).toBe(true);
  });

  it('ignores documented-looking module declarations inside block comments', () => {
    const spoofed = `/*
/// Looks documented but is not Rust code.
pub mod cloud_plan_view;
*/
pub mod cloud_plan_view;
`;
    const documented = `/* unrelated comment */
/// Real module documentation.
pub mod cloud_plan_view;
`;

    expect(publicRustModuleHasOuterDoc(spoofed, 'cloud_plan_view')).toBe(false);
    expect(publicRustModuleHasOuterDoc(documented, 'cloud_plan_view')).toBe(true);
  });
});
