import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readUi(name: string): string {
  return readFileSync(resolve(root, "src/lib", name), "utf8");
}

describe("customer action copy", () => {
  it("does not expose implementation boundaries in cleanup actions", () => {
    const source = [
      readUi("Cleanup.svelte"),
      readUi("CloudArchive.svelte"),
      readUi("Duplicates.svelte"),
      readUi("GitWorktreeCleanup.svelte"),
      readUi("GitCloneCleanup.svelte"),
      readUi("Organize.svelte"),
      readUi("BrewCleanup.svelte"),
      readUi("Inventory.svelte"),
    ].join("\n");

    expect(source).not.toContain("Rust 내부");
    expect(source).not.toContain("Rust 빌드");
    expect(source).not.toContain("backend");
    expect(source).not.toContain("fast-mlsirm");
    expect(source).not.toContain("targetFolder");
    expect(source).not.toContain("온톨로지 정합");
    expect(source).not.toContain("LLM 판정");
    expect(source).not.toContain("model_name");
    expect(source).not.toContain("String(e)");
    expect(source).not.toContain("호스트 이미지");
    expect(source).not.toContain("게스트 저장 공간");
    expect(source).not.toContain("실행 파일을 찾지 못함");
    expect(source).toContain("상태를 다시 확인하세요");
    expect(source).toContain("해상도나 압축이 다른 사진은 자동 삭제하지 않으니 먼저 비교하세요");
  });
});
