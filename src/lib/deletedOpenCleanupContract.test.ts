import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

const component = readFileSync(new URL("./DeletedOpenCleanup.svelte", import.meta.url), "utf8");
const api = readFileSync(new URL("./api.ts", import.meta.url), "utf8");

describe("deleted-open customer action contract", () => {
  it("offers read-only inspection and a normal-quit next action", () => {
    expect(api).toContain('invoke<DeletedOpenActionPlan>("inspect_deleted_open_files")');
    expect(component).toContain("공간을 붙잡고 있는 앱 확인");
    expect(component).toContain("정상 종료한 뒤 다시 확인하세요");
    expect(component).toContain("실제 여유 공간은 종료 후 다시 측정합니다");
  });

  it("does not expose a process termination action or claim physical recovery", () => {
    expect(component).not.toContain("kill(");
    expect(component).not.toContain("terminate_process");
    expect(component).not.toContain("확보했습니다");
    expect(component).toContain("실제 확보량은 아직 측정하지 않았습니다");
  });

  it("keeps audit identifiers behind an expandable detail", () => {
    expect(component).toContain("<details>");
    expect(component).toContain("확인 기록");
    expect(component).toContain("plan.receipt.receipt_id");
  });
});
