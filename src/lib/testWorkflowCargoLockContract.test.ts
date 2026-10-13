import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const workflow = readFileSync(
  new URL("../../.github/workflows/test.yml", import.meta.url),
  "utf8",
);

/** Returns canonical Cargo invocations whose dependency graph must remain lockfile-bound. */
function cargoInvocations(source: string): string[] {
  return source
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => /^cargo\s+(?:test|build|check)\b/.test(line));
}

describe("Test workflow Cargo lockfile contract", () => {
  it("runs every canonical Cargo test/build/check invocation with --locked", () => {
    const commands = cargoInvocations(workflow);

    // Nine canonical invocations currently protect Ubuntu, macOS, Windows and llm-engine lanes.
    // Keep a floor so deleting an exercised Cargo lane cannot make this contract vacuously pass.
    expect(commands.length).toBeGreaterThanOrEqual(9);
    for (const command of commands) {
      expect(command, command).toMatch(/(?:^|\s)--locked(?:\s|$)/);
    }
  });
});
