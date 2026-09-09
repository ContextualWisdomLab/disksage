import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./PodmanEvidence.svelte", import.meta.url), "utf8");
const scriptEnd = source.indexOf("</script>");
const styleStart = source.indexOf("<style>");
const markup = source.slice(scriptEnd + "</script>".length, styleStart > scriptEnd ? styleStart : undefined);
const visibleText = markup.replace(/<[^>]*>/g, " ").replace(/\{[\s\S]*?\}/g, " ");

describe("Podman customer copy contract", () => {
  it("keeps implementation boundaries out of the visible screen", () => {
    for (const term of [
      "증거", "schema", "issue code", "SHA-256", "Raw", "graph root", "VM", "TRIM",
      "dangling", "volume", "provider",
    ]) {
      expect(visibleText.toLowerCase()).not.toContain(term.toLowerCase());
    }
    expect(markup).not.toContain("{error}");
  });

  it("gives visible status and failure text a next action", () => {
    expect(visibleText).toMatch(/확인|다시|시도|판단/);
  });
});
