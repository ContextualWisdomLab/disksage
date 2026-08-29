import type { ExactPhotoGroup, PhotoQuarantinePlan, PhotoQuarantineSelection } from "./api";

export type PhotoCandidateSource = "scan" | "manual";

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

export const syncPhotoCandidatePaths = (
  groups: Array<{ paths: string[] }>,
  currentPaths: string[],
  source: PhotoCandidateSource,
): string[] => source === "manual"
  ? [...currentPaths]
  : groups.flatMap((group) => group.paths);

const normalizedPhotoPath = (value: string): string => {
  const normalized = value.replaceAll("\\", "/").replace(/\/+$/, "");
  return /^[A-Za-z]:\//.test(normalized) ? normalized.toLowerCase() : normalized;
};

export const isPhotoPathWithinRoot = (root: string, path: string): boolean => {
  const normalizedRoot = normalizedPhotoPath(root);
  const normalizedPath = normalizedPhotoPath(path);
  return Boolean(normalizedRoot) && normalizedPath.startsWith(`${normalizedRoot}/`);
};
