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
  it("routes inspection and prune failures through their privacy-safe customer mappers", () => {
    const source = cleanupSource();
    expect(source).toContain(
      'import { podmanEvidenceErrorMessage, podmanPruneErrorMessage } from "./podmanEvidenceError";',
    );
    expect(source).toContain("podmanError = podmanEvidenceErrorMessage(e);");
    expect(source).toContain("podmanPruneError = podmanPruneErrorMessage(e);");
    expect(source).not.toContain("podmanError = String(e);");
    expect(source).not.toContain("podmanPruneError = String(e);");
  });

  it("does not claim the Cleanup screen is mutation-free when dangling-image prune is present", () => {
    const source = cleanupSource();
    expect(source).toContain("dangling 이미지 정리는 정확한 승인 문구와 사유를 입력한 뒤에만 실행됩니다.");
    expect(source).not.toContain("prune, 삭제, trim, 중지는 이 화면에서 실행하지 않습니다.");
  });

  it("keeps the exact destructive approval phrase out of the input placeholder", () => {
    const source = cleanupSource();
    expect(source).not.toContain('placeholder={podmanPlan.dangling_prune_approval_phrase}');
    expect(source).toContain('필요한 승인 문구: <code>{podmanPlan.dangling_prune_approval_phrase}</code>');
    expect(source).toContain('placeholder="승인 문구를 직접 입력하십시오"');
  });
});
