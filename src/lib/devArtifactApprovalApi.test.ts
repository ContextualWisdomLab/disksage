import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));

import * as api from "./api";

const artifact: api.DevArtifact = {
  path: "/repo/target",
  kind: "target",
  project: "repo",
  bytes: 4096,
  allocated_bytes: 4096,
  files: 1,
  skipped: 0,
  scan_complete: true,
  fingerprint: "a".repeat(64),
  object_id: "dev:ino",
  age_days: 0,
};

const approval = {
  selection_fingerprint: "b".repeat(64),
  reviewed_at_ms: 1000,
  expires_at_ms: 301000,
  exact_phrase: `MOVE DEVELOPMENT ARTIFACTS ${"b".repeat(64)} TO TRASH`,
};

describe("development artifact approval API", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
    mocks.invoke.mockReturnValue(Promise.resolve("ok"));
  });

  it("obtains selection-bound backend review authority before cleanup", () => {
    api.reviewDevArtifacts("/repo", [artifact]);
    expect(mocks.invoke).toHaveBeenLastCalledWith("review_dev_artifacts", {
      root: "/repo",
      artifacts: [artifact],
    });
  });

  it("submits the reviewed approval and separately typed exact phrase to the bound cleanup command", () => {
    api.cleanDevArtifactsBound("/repo", 0, [artifact], approval, approval.exact_phrase);
    expect(mocks.invoke).toHaveBeenLastCalledWith("clean_dev_artifacts_bound", {
      root: "/repo",
      minAgeDays: 0,
      artifacts: [artifact],
      approval,
      confirmationPhrase: approval.exact_phrase,
    });
  });

  it("does not expose the legacy boolean-authorized cleanup wrapper", () => {
    expect("cleanDevArtifacts" in api).toBe(false);
  });
});
