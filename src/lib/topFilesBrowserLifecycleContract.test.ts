import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const harness = readFileSync(
  new URL("../../scripts/ci/top-files-browser-e2e.mjs", import.meta.url),
  "utf8",
);

describe("TopFiles browser lifecycle contract", () => {
  it("closes Chrome through browser-level CDP before deleting the profile", () => {
    expect(harness).toContain("/json/version");
    expect(harness).toContain('browserCdp.call("Browser.close")');
    expect(harness).toContain("browser-e2e-browser-target-unavailable");

    const closeRequest = harness.indexOf('browserCdp.call("Browser.close")');
    const profileRemoval = harness.indexOf("rmSync(profile, { recursive: true, force: true })");
    expect(closeRequest).toBeGreaterThan(-1);
    expect(profileRemoval).toBeGreaterThan(closeRequest);

    expect(harness).not.toContain("maxRetries");
    expect(harness).not.toContain("retryDelay");
  });
});
