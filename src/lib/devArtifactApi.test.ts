import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import {
  cleanDevArtifactsBound,
  isDevArtifactApprovalCurrent,
  listDevArtifacts,
  reviewDevArtifacts,
  type DevArtifactApproval,
} from "./devArtifactApi";

describe("development artifact bound approval API", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
  });

  it("treats approval expiry as an exclusive deadline", () => {
    const approval: DevArtifactApproval = {
      selection_fingerprint: "a".repeat(64),
      reviewed_at_ms: 1_000,
      expires_at_ms: 301_000,
      exact_phrase: `MOVE DEVELOPMENT ARTIFACTS ${"a".repeat(64)} TO TRASH`,
    };

    expect(isDevArtifactApprovalCurrent(approval, 300_999)).toBe(true);
    expect(isDevArtifactApprovalCurrent(approval, 301_000)).toBe(false);
    expect(isDevArtifactApprovalCurrent(null, 300_999)).toBe(false);
  });

  it("loads development artifacts without making age an admission authority", async () => {
    mocks.invoke.mockResolvedValueOnce([]);

    await listDevArtifacts("/workspace");

    expect(mocks.invoke).toHaveBeenCalledWith("list_dev_artifacts", {
      root: "/workspace",
      minAgeDays: 0,
    });
  });

  it("forwards review and bound cleanup to the registered Tauri commands", async () => {
    const approval: DevArtifactApproval = {
      selection_fingerprint: "b".repeat(64),
      reviewed_at_ms: 10,
      expires_at_ms: 300_010,
      exact_phrase: `MOVE DEVELOPMENT ARTIFACTS ${"b".repeat(64)} TO TRASH`,
    };
    const artifacts = [];
    mocks.invoke.mockResolvedValueOnce(approval).mockResolvedValueOnce([]);

    await reviewDevArtifacts("/workspace", artifacts);
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "review_dev_artifacts", {
      root: "/workspace",
      artifacts,
    });

    await cleanDevArtifactsBound(
      "/workspace",
      0,
      artifacts,
      approval,
      approval.exact_phrase,
    );
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "clean_dev_artifacts_bound", {
      root: "/workspace",
      minAgeDays: 0,
      artifacts,
      approval,
      confirmationPhrase: approval.exact_phrase,
    });
  });
});
