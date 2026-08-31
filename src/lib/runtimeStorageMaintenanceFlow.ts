export interface RuntimeStorageMutationOutcome<TExecution, TPlan> {
  execution: TExecution;
  plans: TPlan[] | null;
  refreshFailed: boolean;
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
