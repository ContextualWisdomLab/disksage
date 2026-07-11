import { describe, expect, it } from "vitest";
import { blocksDeletion } from "./dupeGuard";
import type { DupeGroup } from "./api";

const g = (hash: string, paths: string[]): DupeGroup => ({ hash, size: 10, paths });

describe("blocksDeletion", () => {
  it("blocks when a group would lose every copy", () => {
    const groups = [g("h1", ["/a", "/b"])];
    expect(blocksDeletion(groups, new Set(["/a", "/b"]))).toBe(true);
  });
  it("allows when each group keeps at least one", () => {
    const groups = [g("h1", ["/a", "/b"]), g("h2", ["/c", "/d"])];
    expect(blocksDeletion(groups, new Set(["/b", "/d"]))).toBe(false);
  });
  it("allows an empty selection", () => {
    expect(blocksDeletion([g("h1", ["/a", "/b"])], new Set())).toBe(false);
  });
  it("blocks if ANY group is fully selected even when others are safe", () => {
    const groups = [g("h1", ["/a", "/b"]), g("h2", ["/c", "/d"])];
    expect(blocksDeletion(groups, new Set(["/b", "/c", "/d"]))).toBe(true);
  });
});
