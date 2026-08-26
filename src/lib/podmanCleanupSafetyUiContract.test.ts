import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("Podman cleanup customer copy", () => {
  it("describes storage checks without exposing machine or image-maintenance jargon", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/Cleanup.svelte"), "utf8");
    const visible = source.slice(source.indexOf("</script>"), source.indexOf("<style>"));

    expect(visible).toContain("Podman 저장 공간");
    expect(visible).toContain("사용 가능한 공간");
    expect(visible).not.toContain("VM 저장소");
    expect(visible).not.toContain("tag가 없는");
    expect(visible).not.toContain("tagged image");
    expect(visible).not.toContain("volume·");
    expect(visible).not.toContain("dangling 이미지");
    expect(visible).not.toContain("지문을 검증합니다");
  });
});
