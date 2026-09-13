import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const llmModulePath = "src-tauri/src/llm/mod.rs";
const llmModelPath = "src-tauri/src/llm/model.rs";
const modelConcurrencyTestPath = "src-tauri/src/llm/model_concurrency_tests.rs";

/** Read one checked-in source file for the LLM model ownership contract. */
function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

/** Detect a root re-export that exposes model implementation-only symbols. */
function hasForbiddenModelRootReexport(source: string): boolean {
  const publicUseStatements = source.match(/^\s*pub\s+use\s+[\s\S]*?;/gm) ?? [];
  return publicUseStatements.some((statement) =>
    /\b(?:self::|crate::llm::)?model\s*::\s*(?:\*|\{[^}]*\b(?:ModelSpec|verify_sha256)\b[^}]*\}|\b(?:ModelSpec|verify_sha256)\b)/s.test(
      statement,
    ),
  );
}

describe("LLM model symbol ownership", () => {
  it("keeps model implementation symbols in model.rs without unused root re-exports", () => {
    const moduleSource = readSource(llmModulePath);
    const modelSource = readSource(llmModelPath);
    const concurrencyTestSource = readSource(modelConcurrencyTestPath);

    expect(modelSource).toContain("pub struct ModelSpec");
    expect(modelSource).toContain("pub fn verify_sha256(");
    expect(hasForbiddenModelRootReexport(moduleSource)).toBe(false);
    expect(concurrencyTestSource).toContain("use super::model::ModelSpec;");
  });

  it("rejects direct, grouped and wildcard model implementation re-exports", () => {
    for (const statement of [
      "pub use model::{download_to, verify_sha256, ModelSpec, DEFAULT};",
      "pub use self::model::{ModelSpec, verify_sha256};",
      "pub use crate::llm::model::ModelSpec;",
      "pub use self::model::*;",
    ]) {
      expect(hasForbiddenModelRootReexport(statement)).toBe(true);
    }

    expect(hasForbiddenModelRootReexport("pub use model::{download_to, DEFAULT};")).toBe(false);
    expect(hasForbiddenModelRootReexport("pub use parse::parse_verdict_full;")).toBe(false);
  });
});
