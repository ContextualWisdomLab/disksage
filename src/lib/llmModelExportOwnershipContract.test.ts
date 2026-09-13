import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const llmModulePath = "src-tauri/src/llm/mod.rs";
const llmModelPath = "src-tauri/src/llm/model.rs";

/** Read one checked-in source file for the LLM model ownership contract. */
function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

/** Detect production root exposure of implementation-only model symbols. */
function hasForbiddenProductionModelRootReexport(source: string): boolean {
  const withoutTestOnlyModelSpec = source.replace(
    /#\[cfg\(test\)\]\s*\npub\s+use\s+(?:self::)?model::ModelSpec\s*;/g,
    "",
  );
  const publicUseStatements = withoutTestOnlyModelSpec.match(/^\s*pub\s+use\s+[\s\S]*?;/gm) ?? [];
  return publicUseStatements.some((statement) =>
    /\b(?:self::|crate::llm::)?model\s*::\s*(?:\*|\{[^}]*\b(?:ModelSpec|verify_sha256)\b[^}]*\}|\b(?:ModelSpec|verify_sha256)\b)/s.test(
      statement,
    ),
  );
}

describe("LLM model symbol ownership", () => {
  it("keeps implementation-only model symbols out of the production LLM root", () => {
    const moduleSource = readSource(llmModulePath);
    const modelSource = readSource(llmModelPath);

    expect(modelSource).toContain("pub struct ModelSpec");
    expect(modelSource).toContain("pub fn verify_sha256(");
    expect(moduleSource).toContain("#[cfg(test)]\npub use model::ModelSpec;");
    expect(hasForbiddenProductionModelRootReexport(moduleSource)).toBe(false);
  });

  it("rejects direct, grouped and wildcard production re-export spellings", () => {
    for (const statement of [
      "pub use model::{download_to, verify_sha256, ModelSpec, DEFAULT};",
      "pub use self::model::{ModelSpec, verify_sha256};",
      "pub use crate::llm::model::ModelSpec;",
      "pub use self::model::*;",
    ]) {
      expect(hasForbiddenProductionModelRootReexport(statement)).toBe(true);
    }

    expect(hasForbiddenProductionModelRootReexport("pub use model::{download_to, DEFAULT};")).toBe(false);
    expect(
      hasForbiddenProductionModelRootReexport("#[cfg(test)]\npub use model::ModelSpec;"),
    ).toBe(false);
    expect(hasForbiddenProductionModelRootReexport("pub use parse::parse_verdict_full;")).toBe(false);
  });
});
