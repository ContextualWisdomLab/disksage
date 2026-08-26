import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

describe("Organize customer copy contract", () => {
  it("keeps implementation-only organization terms out of the customer panel", () => {
    const source = readFileSync(resolve(repositoryRoot, "src/lib/Organize.svelte"), "utf8");
    const visible = source.slice(source.indexOf("</script>"), source.indexOf("<style>"));

    expect(visible).toContain("파일 정리 정보 복사");
    expect(visible).toContain("생산일 확인됨");
    for (const internalTerm of [
      "계보 계약 복사",
      "온톨로지",
      "targetFolder",
      "production_time_source",
      "lineage_fingerprint",
      "String(e)",
    ]) {
      expect(visible).not.toContain(internalTerm);
    }
  });
});
