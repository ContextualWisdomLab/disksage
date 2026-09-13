import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const llmModulePath = "src-tauri/src/llm/mod.rs";
const llmParsePath = "src-tauri/src/llm/parse.rs";

/** Read one canonical Rust source file for the parser ownership contract. */
function readRustSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("LLM parse ownership", () => {
  it("retains the consumed full verdict parser without a dead legacy wrapper", () => {
    const moduleSource = readRustSource(llmModulePath);
    const parseSource = readRustSource(llmParsePath);
    const exportLine = moduleSource
      .split("\n")
      .find((line) => line.startsWith("pub use parse::{"));

    expect(exportLine).toBeDefined();
    expect(exportLine).toContain("parse_verdict_full");
    expect(exportLine).not.toMatch(/\bparse_verdict(?=[,}])/);

    // Production verdict orchestration consumes parse_verdict_full. Keeping a second wrapper only
    // for unit tests would preserve dead production API surface and the compiler warning.
    expect(parseSource).toContain("pub fn parse_verdict_full(");
    expect(parseSource).not.toContain("pub fn parse_verdict(");
  });
});
