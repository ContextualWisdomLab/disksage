import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("BrewCleanup execution status contract", () => {
  it("does not style a non-executed zero-status response as success", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/BrewCleanup.svelte"), "utf8");

    expect(source).toContain(
      "class:success={execution.executed && execution.status_code === 0}",
    );
    expect(source).toContain(
      "class:error={execution.executed && execution.status_code !== 0}",
    );
    expect(source).toContain("실행 성공");
    expect(source).toContain("실행 실패");
    expect(source).toContain("실행되지 않음");
    expect(source).toContain("execution.executed");
    expect(source).not.toContain("class:success={execution.status_code === 0}");
  });
});
