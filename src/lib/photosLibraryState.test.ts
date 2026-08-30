import { describe, expect, it } from "vitest";
import { photosApprovalReady, photosCheckpointCanResume, photosSelections } from "./photosLibraryState";
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

  it("resumes only an incomplete read-only PhotoKit checkpoint", () => {
    expect(photosCheckpointCanResume({ inventory_truncated: true, evidence_complete: false } as PhotosDuplicateInventory)).toBe(true);
    expect(photosCheckpointCanResume({ inventory_truncated: false, evidence_complete: true } as PhotosDuplicateInventory)).toBe(false);
    expect(photosCheckpointCanResume(null)).toBe(false);
  });
});
