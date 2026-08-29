import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

const componentSource = readFileSync(new URL("./PodmanEvidence.svelte", import.meta.url), "utf8");

describe("Podman evidence visual fallback contract", () => {
  it("keeps the panel legible when no global DiskSage design tokens are defined", () => {
    const unresolvedTokenUses = componentSource.match(/var\(--ds-[^,)]+\)/g) ?? [];

    expect(unresolvedTokenUses).toEqual([]);
    expect(componentSource).toContain("var(--ds-border, #ddd)");
    expect(componentSource).toContain("var(--ds-text-muted, #666)");
    expect(componentSource).toContain("var(--ds-success-text, #2a8f4a)");
    expect(componentSource).toContain("var(--ds-warning-text, #8a6508)");
    expect(componentSource).toContain("var(--ds-warning-surface, #fff8e1)");
    expect(componentSource).toContain("var(--ds-danger-text, #b00)");
    expect(componentSource).toContain("var(--ds-control-min-size, 2.75rem)");
    expect(componentSource).toContain("button:focus-visible");
  });
});
