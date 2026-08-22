import { expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));

import { cleanDevArtifacts, type DevArtifact } from "./api";

it("forwards developer artifact cleanup with exact authorization inputs", () => {
  const result = Promise.resolve([]);
  mocks.invoke.mockReturnValue(result);
  const artifact: DevArtifact = {
    path: "/repo/target",
    kind: "rust-target",
    project: "/repo",
    bytes: 4_096,
    files: 8,
    skipped: 0,
    scan_complete: true,
    fingerprint: "a".repeat(64),
    object_id: "b".repeat(64),
    age_days: 45,
  };

  expect(cleanDevArtifacts("/repo", 30, [artifact])).toBe(result);
  expect(mocks.invoke).toHaveBeenCalledWith("clean_dev_artifacts", {
    root: "/repo",
    minAgeDays: 30,
    artifacts: [artifact],
  });
});
