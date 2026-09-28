import { describe, expect, it, vi } from "vitest";
import {
  executeRuntimeStorageMutation,
  runtimeStorageRecoverySucceeded,
} from "./runtimeStorageMaintenanceFlow";

describe("runtime storage post-mutation flow", () => {
  it("preserves a successful mutation receipt and invalidates approval when refresh fails", async () => {
    const execution = { executed: true, marker: "receipt" };
    const invalidateApproval = vi.fn();
    const refresh = vi.fn().mockRejectedValue(new Error("refresh failed"));

    const result = await executeRuntimeStorageMutation(
      async () => execution,
      invalidateApproval,
      refresh,
    );

    expect(result.execution).toBe(execution);
    expect(result.plans).toBeNull();
    expect(result.refreshFailed).toBe(true);
    expect(invalidateApproval).toHaveBeenCalledTimes(1);
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(invalidateApproval.mock.invocationCallOrder[0]).toBeLessThan(
      refresh.mock.invocationCallOrder[0],
    );
  });

  it("does not invalidate approval when the mutation itself fails", async () => {
    const invalidateApproval = vi.fn();
    const refresh = vi.fn();

    await expect(
      executeRuntimeStorageMutation(
        async () => {
          throw new Error("mutation failed");
        },
        invalidateApproval,
        refresh,
      ),
    ).rejects.toThrow("mutation failed");

    expect(invalidateApproval).not.toHaveBeenCalled();
    expect(refresh).not.toHaveBeenCalled();
  });
});

describe("runtime storage recovery outcome", () => {
  it("does not report recovery success when restart completed but the guest is still unreachable", () => {
    expect(
      runtimeStorageRecoverySucceeded({
        executed: true,
        guest_reachable_after_recovery: false,
      }),
    ).toBe(false);
  });

  it("reports recovery success only after a completed restart and a reachable guest", () => {
    expect(
      runtimeStorageRecoverySucceeded({
        executed: true,
        guest_reachable_after_recovery: true,
      }),
    ).toBe(true);
    expect(
      runtimeStorageRecoverySucceeded({
        executed: false,
        guest_reachable_after_recovery: true,
      }),
    ).toBe(false);
  });
});
