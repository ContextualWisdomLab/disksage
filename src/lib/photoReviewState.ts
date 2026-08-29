import type { ExactPhotoGroup, PhotoQuarantinePlan, PhotoQuarantineSelection } from "./api";

export const selectionsForGroups = (
  groups: ExactPhotoGroup[],
  keepers: Record<string, string>,
): PhotoQuarantineSelection[] | null => groups.every((group) => Boolean(keepers[group.content_digest]))
  ? groups.map((group) => ({
      group_fingerprint: group.content_digest,
      survivor_relative_path: keepers[group.content_digest],
    }))
  : null;

export const quarantineApprovalReady = (
  plan: PhotoQuarantinePlan | null,
  typedPhrase: string,
  rationale: string,
): boolean => Boolean(plan && typedPhrase === plan.exact_approval_phrase && rationale.trim());
