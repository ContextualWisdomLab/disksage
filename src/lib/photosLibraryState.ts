import type { PhotosDeletionPlan, PhotosDuplicateInventory, PhotosKeeperSelection } from "./api";

export const photosSelections = (
  inventory: PhotosDuplicateInventory,
  keepers: Record<string, string>,
): PhotosKeeperSelection[] | null => {
  const selections = inventory.exact_groups.map((group) => ({
    content_sha256: group.content_sha256,
    keeper_local_identifier: keepers[group.content_sha256] ?? "",
  }));
  return selections.every((selection) => selection.keeper_local_identifier) ? selections : null;
};

export const photosApprovalReady = (plan: PhotosDeletionPlan, approval: string, rationale: string) =>
  approval === plan.exact_approval_phrase && rationale.trim().length > 0;

export const photosAuthorizationAfterInspectionFailure = (current: string, reason: unknown) =>
  String(reason).includes("photos-authorization-required") ? "unavailable" : current;
