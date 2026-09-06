import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("cloud local eviction customer copy", () => {
  it("does not expose implementation APIs or describe eviction as deletion", () => {
    const source = readFileSync(new URL("./IcloudLocalEviction.svelte", import.meta.url), "utf8");
    for (const internalTerm of ["NSFileProviderManager", "fileproviderctl", "evictItem", "ubiquitous identity"]) {
      expect(source).not.toContain(internalTerm);
    }
    expect(source).not.toContain("로컬 파일만 휴지통으로 이동");
  });
});
