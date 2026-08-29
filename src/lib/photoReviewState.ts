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

const manualAuthorityRoot = (path: string): string | null => {
  const normalized = path.replaceAll("\\", "/");
  const drive = normalized.match(/^([A-Za-z]:)(?:\/|$)/);
  if (drive) return drive[1].toLowerCase();
  if (normalized.startsWith("//")) {
    const parts = normalized.slice(2).split("/").filter(Boolean);
    return parts.length >= 2 ? `//${parts[0].toLowerCase()}/${parts[1].toLowerCase()}` : null;
  }
  return normalized.startsWith("/") ? "/" : null;
};

export const manualPhotoSelectionCompatible = (paths: string[]): boolean => {
  if (paths.length === 0) return false;
  const root = manualAuthorityRoot(paths[0]);
  return root !== null && paths.every((path) => manualAuthorityRoot(path) === root);
};
