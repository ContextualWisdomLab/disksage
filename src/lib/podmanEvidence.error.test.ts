import { describe, expect, it } from "vitest";

import { podmanEvidenceErrorMessage } from "./podmanEvidenceError";

describe("podmanEvidenceErrorMessage", () => {
  it.each([
    new Error("podman failed at /Users/alice/.local/share/containers"),
    "transport error: private-machine.sock",
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
});
