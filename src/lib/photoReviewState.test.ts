import { describe, expect, it } from "vitest";
import {
  manualPhotoSelectionCompatible,
  quarantineApprovalReady,
  selectionsForGroups,
  syncPhotoCandidatePaths,
} from "./photoReviewState";
import type { ExactPhotoGroup, PhotoQuarantinePlan } from "./api";

const group = (digest: string): ExactPhotoGroup => ({
  content_digest: digest, grouping_basis: "decoded-pixel-rgba16-exact", members: [],
  keeper_path: null, keeper_blocker: "selection-required",
});
const plan = { exact_approval_phrase: "DiskSage 승인 exact", plan_fingerprint: "p" } as PhotoQuarantinePlan;

describe("photo review interaction state", () => {
  it("blocks a plan until every ambiguous group has a keeper", () => {
    expect(selectionsForGroups([group("a"), group("b")], { a: "keep-a.png" })).toBeNull();
    expect(selectionsForGroups([group("a"), group("b")], { a: "keep-a.png", b: "keep-b.png" }))
      .toEqual([
        { group_fingerprint: "a", survivor_relative_path: "keep-a.png" },
        { group_fingerprint: "b", survivor_relative_path: "keep-b.png" },
      ]);
  });

  it("enables execution only for exact manual approval plus a reason", () => {
    expect(quarantineApprovalReady(plan, "", "reviewed")).toBe(false);
    expect(quarantineApprovalReady(plan, "DiskSage 승인 exact ", "reviewed")).toBe(false);
    expect(quarantineApprovalReady(plan, "DiskSage 승인 exact", "  ")).toBe(false);
    expect(quarantineApprovalReady(plan, "DiskSage 승인 exact", "사본 검토 완료")).toBe(true);
  });

  it("refreshes scan-owned candidates after a rescan but preserves an explicit manual set", () => {
    const first = [{ paths: ["/scan/a.png", "/scan/b.png"] }];
    const second = [{ paths: ["/scan/c.png", "/scan/d.png"] }];

    expect(syncPhotoCandidatePaths(first, [], "scan")).toEqual(["/scan/a.png", "/scan/b.png"]);
    expect(syncPhotoCandidatePaths(second, ["/scan/a.png", "/scan/b.png"], "scan"))
      .toEqual(["/scan/c.png", "/scan/d.png"]);
    expect(syncPhotoCandidatePaths(second, ["/scan/manual-1.png", "/scan/manual-2.png"], "manual"))
      .toEqual(["/scan/manual-1.png", "/scan/manual-2.png"]);
  });

  it("rejects manual photo sets that cannot share one filesystem authority root", () => {
    expect(manualPhotoSelectionCompatible(["C:\\photos\\a.png", "C:\\elsewhere\\b.png"])).toBe(true);
    expect(manualPhotoSelectionCompatible(["C:\\photos\\a.png", "D:\\photos\\b.png"])).toBe(false);
    expect(manualPhotoSelectionCompatible(["\\\\server\\share\\a.png", "\\\\server\\share\\nested\\b.png"])).toBe(true);
    expect(manualPhotoSelectionCompatible(["\\\\server\\share-a\\a.png", "\\\\server\\share-b\\b.png"])).toBe(false);
    expect(manualPhotoSelectionCompatible(["/photos/a.png", "/other/b.png"])).toBe(true);
    expect(manualPhotoSelectionCompatible(["relative/a.png", "relative/b.png"])).toBe(false);
  });
});
