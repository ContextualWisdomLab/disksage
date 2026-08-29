import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("provider scan admission", () => {
  it("does not publish a rejected provider root to cleanup flows", () => {
    const source = readFileSync("src/routes/+page.svelte", "utf8");
    const admission = source.indexOf("const admittedNode = await api.getNode(selectedRoot);");
    const publication = source.indexOf("crumbs = [selectedRoot];");
    const rejection = source.indexOf("crumbs = [];", publication);

    expect(admission).toBeGreaterThan(-1);
    expect(publication).toBeGreaterThan(admission);
    expect(rejection).toBeGreaterThan(publication);
    expect(source).toContain("<Cleanup scannedRoot={crumbs.length > 0 ? crumbs[0] : null}");
  });
});
