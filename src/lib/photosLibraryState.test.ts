import { describe, expect, it } from "vitest";
import { photosApprovalReady, photosSelections } from "./photosLibraryState";
import type { PhotosDuplicateInventory } from "./api";

const inventory = {
  exact_groups: [{ content_sha256: "digest", members: [], keeper_required: true, automatic_delete_allowed: false }],
} as unknown as PhotosDuplicateInventory;

describe("Apple Photos review state", () => {
  it("requires one explicit keeper and exact fresh approval inputs", () => {
    expect(photosSelections(inventory, {})).toBeNull();
    expect(photosSelections(inventory, { digest: "asset-1" })).toEqual([
      { content_sha256: "digest", keeper_local_identifier: "asset-1" },
    ]);
    const plan = { exact_approval_phrase: "DELETE 1 PHOTOS FROM PHOTOS" } as never;
    expect(photosApprovalReady(plan, "DELETE 1 PHOTOS FROM PHOTOS", "same photo")).toBe(true);
    expect(photosApprovalReady(plan, "DELETE", "same photo")).toBe(false);
  });

});
