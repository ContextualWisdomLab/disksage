import { describe, it, expect } from "vitest";
import { verdictBadge } from "./verdictBadge";

describe("verdictBadge", () => {
  it("maps each verdict to a distinct label", () => {
    expect(verdictBadge("safe").label).toBe("안전");
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
