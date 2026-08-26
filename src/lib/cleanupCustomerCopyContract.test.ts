import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("cleanup customer copy", () => {
  it("keeps implementation vocabulary out of the customer action panel", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/Cleanup.svelte"), "utf8");
    const visible = source.slice(source.indexOf("</script>"), source.indexOf("<style>"));

    for (const term of [
      "온톨로지",
      "계보",
      "attestation",
      "File Provider",
      "OAuth",
      "active-use",
      "APFS 공유 블록",
      "메타데이터 스캔",
      "dangling 이미지",
      "VM 저장소",
    ]) {
      expect(visible).not.toContain(term);
    }
  });

  it("turns failures into a next step instead of exposing raw runtime errors", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/Cleanup.svelte"), "utf8");

    expect(source).not.toContain("loadError = String(e)");
    expect(source).not.toContain("podmanError = String(e)");
    expect(source).not.toContain("podmanPruneError = String(e)");
    expect(source).toContain("다시 시도하십시오");
    expect(source).toContain("최신 상태를 확인한 뒤 다시 시도하십시오");
  });
});
