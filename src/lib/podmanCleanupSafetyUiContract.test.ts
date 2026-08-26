import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("Podman cleanup customer copy contract", () => {
  it("keeps storage guidance actionable without exposing image internals", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/Cleanup.svelte"), "utf8");
    const visible = source.slice(source.indexOf("</script>"), source.indexOf("<style>"));

    expect(visible).toContain("Podman 저장 공간");
    expect(visible).toContain("사용 가능한 공간");
    for (const internalTerm of ["VM 저장소", "tag가 없는", "tagged image", "volume·", "dangling 이미지", "지문을 검증합니다"]) {
      expect(visible).not.toContain(internalTerm);
    }
  });
});
