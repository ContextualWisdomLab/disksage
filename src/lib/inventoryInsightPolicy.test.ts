import { describe, expect, it, vi } from "vitest";
import {
  isCurrentInventoryRequest,
  requestUnknownExtensionInsights,
} from "./inventoryInsightPolicy";

describe("Inventory unknown-extension insight admission", () => {
  it("does not invoke advisory reasoning when the inventory has no unknown samples", async () => {
    const reason = vi.fn(async (_samples: string[]) => [{ ext: "bin" }]);

    const result = await requestUnknownExtensionInsights([], reason);

    expect(result).toBeNull();
    expect(reason).not.toHaveBeenCalled();
  });

  it("forwards the exact bounded sample set when unknown samples exist", async () => {
    const samples = ["/private/sample-a.bin", "/private/sample-b.bin"];
    const reason = vi.fn(async (received: string[]) => received.map((path) => ({ path })));

    const result = await requestUnknownExtensionInsights(samples, reason);

    expect(reason).toHaveBeenCalledTimes(1);
    expect(reason).toHaveBeenCalledWith(samples);
    expect(result).toEqual(samples.map((path) => ({ path })));
  });
});

describe("Inventory request authority", () => {
  it("rejects a response when the scanned root changed while the request was in flight", () => {
    expect(isCurrentInventoryRequest("/disk-a", 7, "/disk-b", 7)).toBe(false);
  });

  it("rejects a response superseded by a newer request for the same root", () => {
    expect(isCurrentInventoryRequest("/disk-a", 7, "/disk-a", 8)).toBe(false);
  });

  it("accepts evidence only when both root identity and generation still match", () => {
    expect(isCurrentInventoryRequest("/disk-a", 7, "/disk-a", 7)).toBe(true);
  });
});
