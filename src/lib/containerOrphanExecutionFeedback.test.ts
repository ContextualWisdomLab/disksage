import { describe, expect, it } from "vitest";
import type { ContainerOrphanPruneExecution } from "./api";
import { containerOrphanExecutionStatus } from "./containerOrphanExecutionFeedback";

function receipt(overrides: Partial<ContainerOrphanPruneExecution> = {}): ContainerOrphanPruneExecution {
  return {
    schema_version: 1,
    runtime_display_name: "container-runtime",
    category: "container",
    candidate_set_sha256: "a".repeat(64),
    command: ["container", "rm", "<candidate-set>"],
    status_code: 0,
    stdout: "",
    stderr: "",
    output_truncated: false,
    executed: true,
    executed_at_ms: 1,
    before_available_bytes: null,
    after_available_bytes: null,
    observed_available_gain_bytes: null,
    rationale: "Reviewed exact evidence.",
    ...overrides,
  };
}

describe("containerOrphanExecutionStatus", () => {
  it("reports a zero-status receipt as complete", () => {
    expect(containerOrphanExecutionStatus(receipt())).toBe("완료");
  });

  it("does not call a non-zero exact-delete receipt a clean failure when partial mutation is possible", () => {
    const message = containerOrphanExecutionStatus(receipt({
      status_code: 1,
      executed: true,
      stderr: "container-orphan-prune-outcome-indeterminate",
    }));

    expect(message).toContain("결과 불확정(1)");
    expect(message).toContain("일부 대상은 이미 제거되었을 수");
    expect(message).toContain("다시 확인");
    expect(message).not.toBe("실패(1)");
  });

  it("keeps an unknown sanitized failure bounded and non-reflecting", () => {
    expect(containerOrphanExecutionStatus(receipt({
      status_code: 17,
      executed: false,
      stderr: "unknown-safe-code",
    }))).toBe("실패(17)");
  });
});
