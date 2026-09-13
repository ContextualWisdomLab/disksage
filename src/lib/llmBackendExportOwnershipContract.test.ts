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

describe("LLM backend ownership", () => {
  it("keeps backend selection in its module without an unused root re-export", () => {
    const moduleSource = readRustSource(llmModulePath);
    const backendSource = readRustSource(llmBackendPath);

    expect(backendSource).toContain("pub enum Backend");
    expect(backendSource).toContain("pub fn choose_backend(");
    expect(moduleSource).not.toMatch(/^pub use backend::/m);
  });
});
