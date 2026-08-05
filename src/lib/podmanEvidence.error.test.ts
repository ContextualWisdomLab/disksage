import { describe, expect, it } from "vitest";

import { podmanEvidenceErrorMessage } from "./podmanEvidence";

describe("podmanEvidenceErrorMessage", () => {
  it.each([
    new Error("podman failed at /Users/alice/.local/share/containers"),
    "transport error: private-machine.sock",
    { secret: "account-local-context" },
    null,
    undefined,
  ])("returns one stable privacy-safe message for untrusted failure detail %#", (reason) => {
    const message = podmanEvidenceErrorMessage(reason);
    expect(message).toBe("podman-evidence-unavailable");
    expect(message).not.toContain("alice");
    expect(message).not.toContain("private-machine");
    expect(message).not.toContain("account-local-context");
  });
});
