export type ContainerOrphanPruneFlowResult<TExecution, TPlan> = {
  execution: TExecution;
  plans: TPlan[] | null;
  refreshError: unknown | null;
};

/**
 * Runs one already-authorized destructive prune and then attempts a read-only refresh.
 *
 * Mutation failure is allowed to reject so callers can present it as a prune failure.
 * Once mutation returns a receipt, however, a later refresh failure must never erase that
 * receipt or be misreported as a failed mutation. The caller can discard stale plans and
 * surface the refresh problem independently.
 */
export async function executeContainerOrphanPruneFlow<TExecution, TPlan>(
  execute: () => Promise<TExecution>,
  refresh: () => Promise<TPlan[]>,
): Promise<ContainerOrphanPruneFlowResult<TExecution, TPlan>> {
  const execution = await execute();
  try {
    return {
      execution,
      plans: await refresh(),
      refreshError: null,
    };
  } catch (refreshError) {
    return {
      execution,
      plans: null,
      refreshError,
    };
  }
}
