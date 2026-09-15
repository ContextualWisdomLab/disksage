import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const workflow = readFileSync(
  new URL("../../.github/workflows/test.yml", import.meta.url),
  "utf8",
);

function namedStep(source: string, name: string): { block: string; index: number } {
  const lines = source.split(/\r?\n/);
  const marker = `- name: ${name}`;
  const start = lines.findIndex((line) => line.trim() === marker);
  if (start < 0) return { block: "", index: -1 };

  const indent = lines[start].length - lines[start].trimStart().length;
  let end = lines.length;
  for (let index = start + 1; index < lines.length; index += 1) {
    const line = lines[index];
    if (!line.trim()) continue;
    const currentIndent = line.length - line.trimStart().length;
    if (currentIndent === indent && line.trimStart().startsWith("- ")) {
      end = index;
      break;
    }
  }
  return { block: lines.slice(start, end).join("\n"), index: start };
}

describe("Test workflow npm failure evidence", () => {
  it("retains the authoritative npm-test transcript before diagnostic reruns", () => {
    const npmTest = namedStep(workflow, "Run npm test");
    expect(npmTest.index).toBeGreaterThanOrEqual(0);
    expect(npmTest.block).toContain("id: npm_test");
    expect(npmTest.block).toContain("continue-on-error: true");
    expect(npmTest.block).toContain("set -o pipefail");
    expect(npmTest.block).toContain(
      'npm test 2>&1 | tee "$RUNNER_TEMP/disksage-npm-test.log"',
    );

    const upload = namedStep(workflow, "Upload authoritative npm test failure transcript");
    expect(upload.index).toBeGreaterThan(npmTest.index);
    expect(upload.block).toContain("if: steps.npm_test.outcome == 'failure'");
    expect(upload.block).toContain(
      "uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
    );
    expect(upload.block).toContain(
      "name: npm-test-failure-${{ github.run_id }}-${{ github.run_attempt }}",
    );
    expect(upload.block).toContain(
      'path: ${{ runner.temp }}/disksage-npm-test.log',
    );
    expect(upload.block).toContain("if-no-files-found: error");

    const firstDiagnostic = namedStep(workflow, "Diagnose SvelteKit sync after npm test failure");
    expect(firstDiagnostic.index).toBeGreaterThan(upload.index);
    const preserve = namedStep(workflow, "Preserve npm test failure");
    expect(preserve.index).toBeGreaterThan(firstDiagnostic.index);
    expect(preserve.block).toContain("if: steps.npm_test.outcome == 'failure'");
    expect(preserve.block).toContain("run: exit 1");
  });
});
