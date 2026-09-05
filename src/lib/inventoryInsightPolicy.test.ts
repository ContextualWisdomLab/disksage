import { describe, expect, it, vi } from "vitest";
import {
  inventoryFailureMessage,
  isCurrentInventoryRequest,
  requestUnknownExtensionInsights,
  type InventoryFailureKind,
} from "./inventoryInsightPolicy";

const inventoryFailureKinds: readonly InventoryFailureKind[] = [
  "inventory-load",
  "ontology-coherence",
  "user-rules",
  "model-status",
  "model-download",
  "unknown-extension-insight",
  "unknown-summary",
];

describe("Inventory failure privacy", () => {
  it.each(inventoryFailureKinds)(
    "never reflects backend causes through the %s customer message",
    (kind) => {
      const privatePath = "/Users/alice/Documents/customer-secret.txt";
      const secret = `backend-sentinel-${kind}`;
      const fromError = inventoryFailureMessage(kind, new Error(`${secret}:${privatePath}`));
      const fromObject = inventoryFailureMessage(kind, {
        message: `${secret}:${privatePath}`,
        token: secret,
      });

      for (const message of [fromError, fromObject]) {
        expect(message.length).toBeGreaterThan(0);
        expect(message).not.toContain(secret);
        expect(message).not.toContain(privatePath);
        expect(message).not.toContain("alice");
      }
      expect(fromError).toBe(fromObject);
    },
  );
});

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
