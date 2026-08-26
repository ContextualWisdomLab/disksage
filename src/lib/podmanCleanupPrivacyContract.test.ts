import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/** Read the production Cleanup component so privacy-safe error wiring cannot silently regress. */
function cleanupSource(): string {
  return readFileSync(resolve(repositoryRoot, "src/lib/Cleanup.svelte"), "utf8");
}

describe("Cleanup Podman privacy and authority copy", () => {
  it("routes Podman failures through the tested privacy mapper", () => {
    const source = cleanupSource();
    expect(source).toContain('import { podmanEvidenceErrorMessage } from "./podmanEvidenceError";');
    expect(source).toContain("podmanError = podmanEvidenceErrorMessage(e);");
    expect(source).toContain("podmanPruneError = podmanEvidenceErrorMessage(e);");
    expect(source).not.toContain("podmanError = String(e);");
    expect(source).not.toContain("podmanPruneError = String(e);");
  });

  it("does not claim the Cleanup screen is mutation-free when dangling-image prune is present", () => {
    const source = cleanupSource();
    expect(source).toContain("dangling 이미지 정리는 정확한 승인 문구와 사유를 입력한 뒤에만 실행됩니다.");
    expect(source).not.toContain("prune, 삭제, trim, 중지는 이 화면에서 실행하지 않습니다.");
  });
});
