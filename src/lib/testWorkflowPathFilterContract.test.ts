import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const workflow = readFileSync(resolve(repositoryRoot, ".github/workflows/test.yml"), "utf8");

function scalarValue(raw: string): string {
  const value = raw.trim();
  if (value.startsWith('"')) {
    const end = value.indexOf('"', 1);
    return end >= 0 ? value.slice(1, end) : value.slice(1);
  }
  if (value.startsWith("'")) {
    const end = value.indexOf("'", 1);
    return end >= 0 ? value.slice(1, end) : value.slice(1);
  }
  return value.split(/\s+#/, 1)[0].trim();
}

function negativePathsIgnoreEntries(source: string): string[] {
  const negatives: string[] = [];
  const lines = source.split(/\r?\n/);

  for (let index = 0; index < lines.length; index += 1) {
    const key = lines[index].match(/^(\s*)paths-ignore:\s*(.*)$/);
    if (!key) continue;

    const keyIndent = key[1].length;
    const inline = key[2].trim();
    if (inline) {
      const listBody = inline.startsWith("[") && inline.endsWith("]")
        ? inline.slice(1, -1)
        : inline;
      for (const rawItem of listBody.split(",")) {
        const value = scalarValue(rawItem);
        if (value.startsWith("!")) negatives.push(value);
      }
      continue;
    }

    for (let cursor = index + 1; cursor < lines.length; cursor += 1) {
      const line = lines[cursor];
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#")) continue;

      const indent = line.length - line.trimStart().length;
      if (indent <= keyIndent) break;

      const listItem = trimmed.match(/^-\s*(.+)$/);
      if (!listItem) continue;
      const value = scalarValue(listItem[1]);
      if (value.startsWith("!")) negatives.push(value);
    }
  }

  return negatives;
}

describe("test workflow path-filter contract", () => {
  it("detects negative paths-ignore entries after comments and in inline lists", () => {
    const fixtures = [
      `pull_request:\n  paths-ignore:\n    - "docs/**"\n    # contract exception\n    - "!docs/example.md"\n`,
      `push:\n  paths-ignore: ["docs/**", "!docs/example.md"]\n`,
    ];

    for (const fixture of fixtures) {
      expect(negativePathsIgnoreEntries(fixture)).toContain("!docs/example.md");
    }
  });

  it("does not put negative globs under paths-ignore", () => {
    expect(negativePathsIgnoreEntries(workflow)).toEqual([]);
  });

  it("runs the Windows agent-state regression when that owner source is present", () => {
    expect(workflow).toContain("Test-Path 'src-tauri/src/agent_state_guard.rs'");
    expect(workflow).toContain(
      "rustc --edition=2021 --test src-tauri/src/agent_state_guard.rs -o target/agent-state-guard.exe",
    );
    expect(workflow).toContain("& .\\target\\agent-state-guard.exe --nocapture");
  });
});
