import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("stale clone cleanup warning contract", () => {
  it("keeps completed Trash movement distinct from incomplete follow-up evidence", () => {
    const component = readFileSync("src/lib/GitCloneCleanup.svelte", "utf8");
    const api = readFileSync("src/lib/api.ts", "utf8");

    expect(api).toContain("post_mutation_warning: string | null");
    expect(component).toContain("removal.result.post_mutation_warning");
    expect(component).toContain("휴지통을 비우지 말고 다시 확인하세요.");
    expect(component).not.toContain("terminal audit");
  });
});
