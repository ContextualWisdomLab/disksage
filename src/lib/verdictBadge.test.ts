import { describe, expect, it } from "vitest";
import { verdictBadge } from "./verdictBadge";

describe("verdictBadge", () => {
  it("keeps model advice distinct without presenting safe as deletion authorization", () => {
    const safe = verdictBadge("safe");
    expect(safe.label).toBe("낮은 위험");
    expect(safe.title).toContain("자문");
    expect(safe.title).toContain("검증");
    expect(safe.title).not.toContain("삭제해도 안전");

    expect(verdictBadge("caution").label).toBe("주의");
    expect(verdictBadge("keep").label).toBe("보관");
    expect(verdictBadge("unrated").label).toBe("미판정");
  });
  it("gives distinct css classes", () => {
    const classes = ["safe", "caution", "keep", "unrated"].map((v) => verdictBadge(v as any).cls);
    expect(new Set(classes).size).toBe(4);
  });
  it("falls back to the unrated badge for unknown input", () => {
    expect(verdictBadge("bogus" as any).label).toBe("미판정");
    expect(verdictBadge("bogus" as any).cls).toBe(verdictBadge("unrated").cls);
  });
});
