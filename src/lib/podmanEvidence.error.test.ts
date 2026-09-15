import { describe, expect, it } from "vitest";

import { podmanEvidenceErrorMessage, podmanPruneErrorMessage } from "./podmanEvidenceError";

describe("podmanEvidenceErrorMessage", () => {
  it.each([
    new Error("podman failed at /Users/alice/.local/share/containers"),
    "transport error: private-machine.sock",
    "toString",
    "constructor",
    "__proto__",
    { secret: "account-local-context" },
    null,
    undefined,
  ])("returns one stable privacy-safe message for untrusted failure detail %#", (reason) => {
    const message = podmanEvidenceErrorMessage(reason);
    expect(message).toBe("Podman 저장 공간을 확인하지 못했습니다. 상태를 확인한 뒤 다시 시도하십시오.");
    expect(message).not.toContain("alice");
    expect(message).not.toContain("private-machine");
    expect(message).not.toContain("account-local-context");
    expect(message).not.toContain("podman-evidence");
    expect(message).toContain("다시 시도하십시오");
  });

  it.each([
    "podman-prune-confirmation-mismatch",
    "podman-prune-candidate-set-changed",
    "podman-prune-machine-not-running",
  ])("does not surface prune-only recovery guidance during read-only inspection for %s", (reason) => {
    expect(podmanEvidenceErrorMessage(reason)).toBe(
      "Podman 저장 공간을 확인하지 못했습니다. 상태를 확인한 뒤 다시 시도하십시오.",
    );
  });
});

describe("podmanPruneErrorMessage", () => {
  it.each([
    [
      "podman-prune-confirmation-mismatch",
      "승인 문구가 최신 정리 계획과 일치하지 않습니다. 현재 계획을 다시 확인한 뒤 승인 문구를 다시 입력하십시오.",
    ],
    [
      "podman-prune-candidate-set-changed",
      "정리 후보가 변경되었습니다. 최신 Podman 상태를 다시 확인하고 새 계획을 검토하십시오.",
    ],
    [
      "podman-prune-machine-not-running",
      "Podman 머신이 실행 중이 아닙니다. 머신 상태를 확인한 뒤 정리 계획을 다시 불러오십시오.",
    ],
  ])("maps stable prune code %s to bounded recovery guidance", (reason, expected) => {
    expect(podmanPruneErrorMessage(reason)).toBe(expected);
  });

  it.each([
    new Error("podman-prune-candidate-set-changed: /Users/alice/private"),
    "socket private-machine.sock failed",
    "toString",
    "constructor",
    "__proto__",
    { reason: "podman-prune-confirmation-mismatch", secret: "account-local-context" },
    null,
    undefined,
  ])("does not reflect untrusted prune failure detail %#", (reason) => {
    const message = podmanPruneErrorMessage(reason);
    expect(message).toBe(
      "Podman 정리를 완료하지 못했습니다. 최신 상태를 다시 확인한 뒤 정리 계획을 재검토하십시오.",
    );
    expect(message).not.toContain("alice");
    expect(message).not.toContain("private-machine");
    expect(message).not.toContain("account-local-context");
    expect(message).not.toContain("/Users/");
    expect(message.length).toBeLessThanOrEqual(120);
  });
});
