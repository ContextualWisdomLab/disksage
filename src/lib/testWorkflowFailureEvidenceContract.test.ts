import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const workflow = readFileSync(
  new URL("../../.github/workflows/test.yml", import.meta.url),
  "utf8",
);

describe("Test workflow npm failure evidence", () => {
  it("retains the authoritative npm-test transcript before diagnostic reruns", () => {
    expect(workflow).toContain("set -o pipefail");
    expect(workflow).toContain(
      'npm test 2>&1 | tee "$RUNNER_TEMP/disksage-npm-test.log"',
    );
    expect(workflow).toContain("if: steps.npm_test.outcome == 'failure'");
    expect(workflow).toContain(
      "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
    );
    expect(workflow).toContain(
      "name: npm-test-failure-${{ github.run_id }}-${{ github.run_attempt }}",
    );
    expect(workflow).toContain(
      'path: ${{ runner.temp }}/disksage-npm-test.log',
    );
    expect(workflow).toContain("if-no-files-found: error");
    expect(workflow).toContain("- name: Preserve npm test failure");
  });
});
