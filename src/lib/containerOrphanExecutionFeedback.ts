import type { ContainerOrphanPruneExecution } from "./api";

export const CONTAINER_PRUNE_OUTCOME_INDETERMINATE =
  "container-orphan-prune-outcome-indeterminate";

export function containerOrphanExecutionStatus(
  execution: ContainerOrphanPruneExecution,
): string {
  if (execution.executed && execution.status_code === 0) return "완료";
  if (execution.stderr === CONTAINER_PRUNE_OUTCOME_INDETERMINATE) {
    return `결과 불확정(${execution.status_code}) · 일부 대상은 이미 제거되었을 수 있으므로 최신 상태를 다시 확인하세요.`;
  }
  return `실패(${execution.status_code})`;
}
