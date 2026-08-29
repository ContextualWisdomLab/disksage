import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("development build-root customer copy", () => {
  it("tells the customer what to do without exposing mutation internals", () => {
    const component = readFileSync(resolve(process.cwd(), "src/lib/Cleanup.svelte"), "utf8");

    expect(component).toContain("개발 도구를 닫고 항목을 선택한 뒤 휴지통으로 보내세요");
    expect(component).toContain("휴지통에서 언제든 복원할 수 있습니다");
    expect(component).not.toContain("메타데이터 지문");
    expect(component).not.toContain("atomic staging");
    expect(component).not.toContain("object_id");
  });
});
