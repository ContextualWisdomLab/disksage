import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const llmModulePath = "src-tauri/src/llm/mod.rs";
const llmBackendPath = "src-tauri/src/llm/backend.rs";

/** Read one checked-in Rust source file for the backend ownership contract. */
function readRustSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

/** Detect a public use statement that exposes backend policy through the LLM root. */
function hasForbiddenBackendRootReexport(source: string): boolean {
  const publicUseStatements = source.match(/^\s*pub\s+use\s+[\s\S]*?;/gm) ?? [];
  return publicUseStatements.some((statement) =>
    /\bbackend\s*::\s*(?:\*|\{[^}]*\b(?:Backend|choose_backend)\b[^}]*\}|\b(?:Backend|choose_backend)\b)/s.test(
      statement,
    ),
  );
}

describe("LLM backend ownership", () => {
  it("keeps backend selection in its module without an unused root re-export", () => {
    const moduleSource = readRustSource(llmModulePath);
    const backendSource = readRustSource(llmBackendPath);

    expect(backendSource).toContain("pub enum Backend");
    expect(backendSource).toContain("pub fn choose_backend(");
    expect(hasForbiddenBackendRootReexport(moduleSource)).toBe(false);
  });

  it("rejects equivalent backend re-export spellings", () => {
    for (const statement of [
      "pub use backend::{Backend, choose_backend};",
      "pub use self::backend::{Backend, choose_backend};",
      "pub use crate::llm::backend::Backend;",
      "pub use self::backend::*;",
    ]) {
      expect(hasForbiddenBackendRootReexport(statement)).toBe(true);
    }

    expect(hasForbiddenBackendRootReexport("pub use parse::parse_verdict_full;")).toBe(false);
  });
});
