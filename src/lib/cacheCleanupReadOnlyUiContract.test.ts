import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("cache cleanup fail-closed UX", () => {
  it("offers only the identity-bound cache action", () => {
    const cleanup = readSource("src/lib/Cleanup.svelte");

    expect(cleanup).toContain("cleanCacheContents");
    expect(cleanup).not.toContain("selectedRules");
    expect(cleanup).toContain('role="status"');
    expect(cleanup).toContain("각 항목의 크기와 수정 시각을 다시 확인합니다");
    expect(cleanup).not.toContain("active-use");
  });

  it("does not render the permanent-delete action when the backend withholds destructive authority", () => {
    const cleanup = readSource("src/lib/Cleanup.svelte");

    expect(cleanup).toContain('cacheTrashPurgeAvailability } from "./cacheTrashPurgeAvailability"');
    expect(cleanup).toContain("cacheTrashPurgeInstruction = purgeAvailability.instruction");
    expect(cleanup).toContain("cacheTrashApprovalPhrase = purgeAvailability.canPurge ? cacheTrashReview.approval_phrase : null");
    expect(cleanup).toContain('import { cacheTrashPurgeItemMessage } from "./cacheTrashPurgeItemMessage"');
    expect(cleanup).toContain("cacheTrashPurgeItemMessage(item)");
    expect(cleanup).toContain("{#if cacheTrashPurgeInstruction}");
    expect(cleanup).toContain("{:else if cacheTrash.length > 0}");
  });

  it("does not imply macOS currently authorizes in-app permanent deletion", () => {
    const cleanup = readSource("src/lib/Cleanup.svelte");
    const availability = readSource("src/lib/cacheTrashPurgeAvailability.ts");

    expect(availability).toContain(
      "휴지통 속 재생성 캐시 검토는 현재 macOS 기본 휴지통에서만 지원합니다. 앱 내 영구 삭제는 안전한 객체 결합 삭제를 제공할 때까지 모든 플랫폼에서 비활성화되어 있습니다.",
    );
    expect(cleanup).not.toContain(
      "휴지통 안의 캐시를 영구 삭제하는 물리 공간 회수 기능은 현재 macOS 기본 휴지통에서만 지원합니다.",
    );
  });
});
