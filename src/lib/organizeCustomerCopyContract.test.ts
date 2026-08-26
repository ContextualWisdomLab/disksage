import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("Organize customer copy", () => {
  it("keeps ontology and lineage implementation details out of customer guidance", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/Organize.svelte"), "utf8");
    const visible = source.slice(source.indexOf("</script>"), source.indexOf("<style>"));

    expect(visible).toContain("파일 정리 정보 복사");
    expect(visible).not.toContain("계보 계약 복사");
    expect(visible).not.toContain("온톨로지");
    expect(visible).not.toContain("targetFolder");
    expect(visible).not.toContain("production_time_source");
    expect(visible).not.toContain("lineage_fingerprint");
    expect(source).not.toContain("String(e)");
  });
});
