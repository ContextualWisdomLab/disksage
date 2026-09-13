import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const workflow = readFileSync(
  new URL("../../.github/workflows/test.yml", import.meta.url),
  "utf8",
);

type NamedStep = {
  block: string;
  start: number;
};

/**
 * Returns the exact named workflow step and its line position so evidence,
 * diagnostics, and failure preservation cannot be reordered silently.
 */
function namedStep(source: string, stepName: string): NamedStep {
  const lines = source.split(/\r?\n/);
  const start = lines.findIndex(
    (line) => line.trim() === `- name: ${stepName}`,
  );
  if (start < 0) {
    throw new Error(`missing workflow step: ${stepName}`);
  }

  const stepIndent = lines[start].match(/^(\s*)/)?.[1] ?? "";
  let end = start + 1;
  while (
    end < lines.length &&
    !lines[end].startsWith(`${stepIndent}- `)
  ) {
    end += 1;
  }

  return {
    block: lines.slice(start, end).join("\n"),
    start,
  };
}

describe("Test workflow Rust failure evidence", () => {
  it("retains the authoritative Rust-test transcript before preserving failure", () => {
    const rustTest = namedStep(workflow, "Rust tests (includes unix symlink test)");
    const upload = namedStep(
      workflow,
      "Upload authoritative Rust test failure transcript",
    );
    const preserve = namedStep(workflow, "Preserve Rust test failure");
    const nextRustLane = namedStep(workflow, "Headless cloud planner tests");

    expect(rustTest.block).toContain("id: rust_test");
    expect(rustTest.block).toContain("continue-on-error: true");
    expect(rustTest.block).toContain("set -o pipefail");
    expect(rustTest.block).toContain(
      'cargo test --locked --manifest-path src-tauri/Cargo.toml 2>&1 | tee "$RUNNER_TEMP/disksage-rust-test.log"',
    );
    expect(upload.block).toContain("if: steps.rust_test.outcome == 'failure'");
    expect(upload.block).toContain(
      "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
    );
    expect(upload.block).toContain(
      "name: rust-test-failure-${{ github.run_id }}-${{ github.run_attempt }}",
    );
    expect(upload.block).toContain(
      'path: ${{ runner.temp }}/disksage-rust-test.log',
    );
    expect(upload.block).toContain("if-no-files-found: error");
    expect(preserve.block).toContain(
      "if: steps.rust_test.outcome == 'failure'",
    );
    expect(preserve.block).toContain("run: exit 1");

    expect(rustTest.start).toBeLessThan(upload.start);
    expect(upload.start).toBeLessThan(preserve.start);
    expect(preserve.start).toBeLessThan(nextRustLane.start);
  });
});

describe("Test workflow npm failure evidence", () => {
  it("retains the authoritative npm-test transcript before diagnostic reruns", () => {
    const npmTest = namedStep(workflow, "Run npm test");
    const upload = namedStep(
      workflow,
      "Upload authoritative npm test failure transcript",
    );
    const diagnostics = [
      "Diagnose SvelteKit sync after npm test failure",
      "Diagnose Vitest after npm test failure",
      "Diagnose workflow contract after npm test failure",
      "Diagnose browser test after npm test failure",
    ].map((name) => namedStep(workflow, name));
    const preserve = namedStep(workflow, "Preserve npm test failure");

    expect(npmTest.block).toContain("set -o pipefail");
    expect(npmTest.block).toContain(
      'npm test 2>&1 | tee "$RUNNER_TEMP/disksage-npm-test.log"',
    );
    expect(upload.block).toContain("if: steps.npm_test.outcome == 'failure'");
    expect(upload.block).toContain(
      "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
    );
    expect(upload.block).toContain(
      "name: npm-test-failure-${{ github.run_id }}-${{ github.run_attempt }}",
    );
    expect(upload.block).toContain(
      'path: ${{ runner.temp }}/disksage-npm-test.log',
    );
    expect(upload.block).toContain("if-no-files-found: error");

    expect(npmTest.start).toBeLessThan(upload.start);
    expect(upload.start).toBeLessThan(diagnostics[0].start);
    for (let index = 1; index < diagnostics.length; index += 1) {
      expect(diagnostics[index - 1].start).toBeLessThan(
        diagnostics[index].start,
      );
    }
    for (const diagnostic of diagnostics) {
      expect(diagnostic.block).toContain(
        "if: steps.npm_test.outcome == 'failure'",
      );
      expect(diagnostic.block).toContain("continue-on-error: true");
      expect(diagnostic.start).toBeLessThan(preserve.start);
    }

    expect(preserve.block).toContain(
      "if: steps.npm_test.outcome == 'failure'",
    );
    expect(preserve.block).toContain("run: exit 1");
    expect(workflow.split(/\r?\n/).findIndex(
      (line) => line.trim() === "- run: npm run build",
    )).toBeGreaterThan(preserve.start);
  });
});
