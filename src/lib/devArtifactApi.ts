import { invoke } from "@tauri-apps/api/core";
import type { CleanResult, DevArtifact } from "./api";

export interface DevArtifactApproval {
  selection_fingerprint: string;
  reviewed_at_ms: number;
  expires_at_ms: number;
  exact_phrase: string;
}

export const reviewDevArtifacts = (root: string, artifacts: DevArtifact[]) =>
  invoke<DevArtifactApproval>("review_dev_artifacts", { root, artifacts });

export const cleanDevArtifactsBound = (
  root: string,
  minAgeDays: number,
  artifacts: DevArtifact[],
  approval: DevArtifactApproval,
  confirmationPhrase: string,
) =>
  invoke<CleanResult[]>("clean_dev_artifacts_bound", {
    root,
    minAgeDays,
    artifacts,
    approval,
    confirmationPhrase,
  });
