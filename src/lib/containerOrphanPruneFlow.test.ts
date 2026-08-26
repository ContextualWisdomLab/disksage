import { describe, expect, it, vi } from "vitest";
import { executeContainerOrphanPruneFlow } from "./containerOrphanPruneFlow";

describe("container orphan prune flow", () => {
  it("never refreshes when the destructive command itself fails", async () => {
    const execute = vi.fn(async () => {
      throw new Error("mutation failed");
    });
    const refresh = vi.fn(async () => ["fresh-plan"]);

    await expect(executeContainerOrphanPruneFlow(execute, refresh)).rejects.toThrow(
      "mutation failed",
    );
    expect(refresh).not.toHaveBeenCalled();
  });

  it("returns the execution receipt together with a successful fresh plan", async () => {
    const receipt = { executed: true, status_code: 0 };
    const plans = [{ runtime: "docker-native" }];

    const result = await executeContainerOrphanPruneFlow(
      async () => receipt,
      async () => plans,
    );

    expect(result).toEqual({
      execution: receipt,
      plans,
      refreshError: null,
    });
  });

  it("preserves a successful mutation receipt when the post-mutation refresh fails", async () => {
    const receipt = { executed: true, status_code: 0 };
    const refreshError = new Error("refresh unavailable");

    const result = await executeContainerOrphanPruneFlow(
      async () => receipt,
      async () => {
        throw refreshError;
      },
    );

    expect(result.execution).toBe(receipt);
    expect(result.plans).toBeNull();
    expect(result.refreshError).toBe(refreshError);
  });
});
