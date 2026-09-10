import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("runtime storage customer copy contract", () => {
  it("keeps recovery and space guidance actionable without implementation terms", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/Cleanup.svelte"), "utf8");
    const visible = source.slice(source.indexOf("</script>"), source.indexOf("<style>"));

    expect(visible).toContain("Podman·Colima 저장 공간 확인");
    expect(visible).toContain("연결을 복구한 뒤 다시 확인하세요");
    expect(visible).toContain("해당 도구의 관리 화면에서 상태를 확인하세요");
    expect(visible).toContain('placeholder="위 확인 문구를 직접 입력하세요"');
    expect(visible).not.toContain("가상 머신");
    expect(visible).not.toContain("런타임 관리");
    expect(visible).not.toContain("게스트");
    expect(source).toContain("runtimeStorageRecoveryReady");
    expect(source).toContain("executeRuntimeStorageRecovery");
    expect(source).not.toContain("String(e)");
  });
});
