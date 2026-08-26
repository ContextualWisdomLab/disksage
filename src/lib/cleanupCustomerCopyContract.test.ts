import { readdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function visibleText(fileName: string): string {
  const source = readFileSync(resolve(repositoryRoot, "src/lib", fileName), "utf8");
  const markup = source.slice(source.indexOf("</script>") + "</script>".length, source.indexOf("<style>"));
  return markup.replace(/<[^>]*>/g, " ").replace(/\{[\s\S]*?\}/g, " ");
}

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
    expect(source).toContain("cacheTrashPurgeAvailability(cacheTrashReview)");
    expect(source).toContain("cacheTrashPurgeInstruction");
    expect(source).toContain("다시 시도하십시오");
    expect(source).toContain("최신 상태를 확인한 뒤 다시 시도하십시오");
  });

  it("applies the implementation-boundary vocabulary rule to every customer screen", () => {
    const forbidden = [
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
      "증거 공백",
      "분리된 HEAD",
      "Git 등록 해제",
      "사전 할당량 기준",
      "tag가 없는",
      "tagged image",
      "지문을 검증합니다",
      "승인 기록",
      "결과 기록",
      "계획 지문",
      "감사 기록",
      "dry-run",
      "LLM",
    ];

    for (const fileName of readdirSync(resolve(repositoryRoot, "src/lib"))) {
      if (!fileName.endsWith(".svelte")) continue;
      const text = visibleText(fileName);
      for (const term of forbidden) expect(text, `${fileName}: ${term}`).not.toContain(term);
    }
  });

  it("keeps inventory and duplicate-file failures actionable", () => {
    for (const fileName of ["Inventory.svelte", "Duplicates.svelte"]) {
      const source = readFileSync(resolve(repositoryRoot, "src/lib", fileName), "utf8");
      expect(source).not.toContain("String(e)");
      expect(source).toContain("다시 시도하십시오");
    }
  });
});
