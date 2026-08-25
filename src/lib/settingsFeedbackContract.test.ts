import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("settings persistence feedback copy and accessibility", () => {
  it("surfaces action-oriented load and save failures instead of silently swallowing them", () => {
    const source = readSource("src/lib/Settings.svelte");

    expect(source).not.toContain(".catch(() => {})");
    expect(source).not.toMatch(/catch\s*\{\s*\}/);
    expect(source).toContain(
      "설정을 불러오지 못했습니다. 안전을 위해 오프라인 모드를 유지합니다. DiskSage 데이터 폴더의 권한을 확인한 뒤 앱을 다시 열어 보세요.",
    );
    expect(source).toContain(
      "설정을 저장하지 못했습니다. 이전 온라인 모드 설정을 유지합니다. DiskSage 데이터 폴더의 권한과 여유 공간을 확인한 뒤 다시 시도하세요.",
    );
  });

  it("keeps persistence feedback accessible and blocks repeat input while work is in flight", () => {
    const source = readSource("src/lib/Settings.svelte");

    expect(source).toContain('role="alert"');
    expect(source).toContain("disabled={busy}");
  });
});
