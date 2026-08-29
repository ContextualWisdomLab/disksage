import type { ExactPhotoGroup, PhotoQuarantinePlan, PhotoQuarantineSelection } from "./api";

export const duplicateCandidatePaths = (groups: { paths: string[] }[]): string[] =>
  groups.flatMap((group) => group.paths);

export const duplicateCandidateFingerprint = (groups: { paths: string[] }[]): string =>
  JSON.stringify(duplicateCandidatePaths(groups));

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
