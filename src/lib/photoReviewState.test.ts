import { describe, expect, it } from "vitest";
import { quarantineApprovalReady, selectionsForGroups } from "./photoReviewState";
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
});
