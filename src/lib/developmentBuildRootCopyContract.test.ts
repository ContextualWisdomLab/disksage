import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("development build-root customer copy", () => {
  it("tells the customer what to do without exposing mutation internals", () => {
    const component = readFileSync(resolve(process.cwd(), "src/lib/Cleanup.svelte"), "utf8");

    expect(component).toContain("정리할 개발 작업공간을 직접 선택하면");
    expect(component).toContain("휴지통에서 복원할 수 있으며");
    expect(component).not.toContain("메타데이터 지문");
    expect(component).not.toContain("atomic staging");
    expect(component).not.toContain("object_id");
  });

  it("uses the selection-bound review and typed confirmation path", () => {
    const component = readFileSync(resolve(process.cwd(), "src/lib/Cleanup.svelte"), "utf8");

    expect(component).toContain("devArtifactApi.reviewDevArtifacts");
    expect(component).toContain("devArtifactApi.cleanDevArtifactsBound");
    expect(component).toContain("bind:value={devArtifactConfirmationPhrase}");
    expect(component).not.toContain("api.cleanDevArtifacts(");
  });

  it("requires an explicit development workspace instead of recursively using the disk scan root", () => {
    const component = readFileSync(resolve(process.cwd(), "src/lib/Cleanup.svelte"), "utf8");

    expect(component).toContain("chooseDevArtifactRoot");
    expect(component).toContain("directory: true");
    expect(component).toContain("devArtifactRoot");
    expect(component).toContain("devArtifactApi.listDevArtifacts(devArtifactRoot)");
    expect(component).not.toContain("devArtifactApi.listDevArtifacts(scannedRoot)");
  });
});