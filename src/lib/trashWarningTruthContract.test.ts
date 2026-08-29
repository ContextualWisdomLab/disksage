import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("completed Trash move warning contract", () => {
  it("keeps a completed cache or development-artifact move successful and shows the next action", () => {
    const api = readSource("src/lib/api.ts");
    const cleanup = readSource("src/lib/Cleanup.svelte");

    expect(api).toMatch(/interface CleanResult[\s\S]*warning: string/);
    expect(cleanup).toContain("r.ok && r.warning");
    expect(cleanup).toContain(
      "이동은 완료됐지만 기록을 확인하지 못했습니다. 휴지통에서 항목을 확인한 뒤 다시 스캔하세요.",
    );
    expect(cleanup).not.toContain("{r.warning}");
  });

  it("keeps a completed orphan move counted while showing a safe recovery action", () => {
    const api = readSource("src/lib/api.ts");
    const component = readSource("src/lib/OrphanCleanup.svelte");

    expect(api).toMatch(/interface OrphanCleanupItemResult[\s\S]*warning: string \| null/);
    expect(component).toContain("item.moved_to_trash && item.warning");
    expect(component).toContain(
      "이동은 완료됐지만 일부 기록을 확인하지 못했습니다. 휴지통에서 항목을 확인한 뒤 고아 관계 조사를 다시 실행하세요.",
    );
    expect(component).not.toContain("{item.warning}");
  });
});
