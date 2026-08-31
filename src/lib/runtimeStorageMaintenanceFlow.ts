export interface RuntimeStorageMutationOutcome<TExecution, TPlan> {
  execution: TExecution;
  plans: TPlan[] | null;
  refreshFailed: boolean;
}

export interface RuntimeStorageRecoveryOutcome {
  executed: boolean;
  guest_reachable_after_recovery: boolean;
}

/**
 * A completed restart is an execution receipt, not proof that the guest recovered.
 * Customer-facing success requires both the completed mutation and a fresh positive
 * reachability observation after the restart.
 */
export function runtimeStorageRecoverySucceeded(
  execution: RuntimeStorageRecoveryOutcome,
): boolean {
  return execution.executed && execution.guest_reachable_after_recovery;
}

/**
 * Preserve a completed maintenance receipt even when the read-only refresh fails.
 * Approval state is invalidated immediately after mutation success, before any
 * fallible post-mutation inspection, so a stale phrase cannot be reused.
 */
export async function executeRuntimeStorageMutation<TExecution, TPlan>(
  execute: () => Promise<TExecution>,
  invalidateApproval: () => void,
  refresh: () => Promise<TPlan[]>,
): Promise<RuntimeStorageMutationOutcome<TExecution, TPlan>> {
  const execution = await execute();
  invalidateApproval();
  try {
    return {
      execution,
      plans: await refresh(),
      refreshFailed: false,
    };
  } catch {
    return {
      execution,
      plans: null,
      refreshFailed: true,
    };
  }
}
