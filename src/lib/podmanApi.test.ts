import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import { podmanReclaimPlan } from "./podmanApi";

describe("Podman desktop API", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
  });

  it("requests the documented default machine by passing a null selector", () => {
    const result = Promise.resolve({ evidence_complete: false });
    mocks.invoke.mockReturnValue(result);

    expect(podmanReclaimPlan()).toBe(result);
    expect(mocks.invoke).toHaveBeenCalledWith("podman_reclaim_plan", { machine: null });
  });

  it("passes an explicit machine as a typed Tauri argument without shell construction", () => {
    const result = Promise.resolve({ evidence_complete: true });
    mocks.invoke.mockReturnValue(result);

    expect(podmanReclaimPlan("engineering-machine")).toBe(result);
    expect(mocks.invoke).toHaveBeenCalledWith("podman_reclaim_plan", {
      machine: "engineering-machine",
    });
  });
});
