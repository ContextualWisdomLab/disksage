import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import * as api from "./api";

describe("development artifact API authority", () => {
  it("does not export the retired boolean-approval cleanup route", () => {
    expect(Object.prototype.hasOwnProperty.call(api, "cleanDevArtifacts")).toBe(false);
  });
});
