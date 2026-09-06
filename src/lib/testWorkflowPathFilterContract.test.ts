import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
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

// Exercise the canonical shell admission without compiling or faking Rust test results.
it("macOS cache job executes present owner tests, reports absent source, and propagates failure", () => {
  const job = workflow.split("  macos-cache-cleanup:\n")[1]?.split("  windows-home-resolution:")[0] ?? "";
  expect(job).toContain("runs-on: macos-latest");
  expect(job).toContain("ref: ${{ github.event.pull_request.head.sha || github.sha }}");
  const script = job.match(/        run: \|\n([\s\S]*)/)?.[1].replace(/^          /gm, "") ?? "";
  for (const target of ["cache_cleanup_corepack_scope", "cache_cleanup_cli_permanent_gradle", "generated_cache_staged_activity"]) {
    expect(script).toContain(target);
  }
  const fixture = mkdtempSync(resolve(tmpdir(), "disksage-workflow-admission-"));
  try {
    const bin = resolve(fixture, "bin");
    mkdirSync(bin);
    const log = resolve(fixture, "cargo.log");
    writeFileSync(resolve(bin, "cargo"), "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> \"$CARGO_LOG\"\nexit \"${CARGO_EXIT:-0}\"\n", { mode: 0o700 });
    const env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, CARGO_LOG: log };
    const run = (extra = {}) => spawnSync("bash", ["-e", "-c", script], { cwd: fixture, env: { ...env, ...extra }, encoding: "utf8" });
    const absent = run();
    expect(absent.status).toBe(0);
    expect(absent.stdout.match(/no runtime regression executed/g)).toHaveLength(3);
    expect(existsSync(log)).toBe(false);
    mkdirSync(resolve(fixture, "src-tauri/tests"), { recursive: true });
    writeFileSync(resolve(fixture, "src-tauri/tests/generated_cache_staged_activity.rs"), "");
    expect(run().status).toBe(0);
    expect(readFileSync(log, "utf8")).toBe("test --manifest-path src-tauri/Cargo.toml --test generated_cache_staged_activity\n");
    expect(run({ CARGO_EXIT: "7" }).status).toBe(7);
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
});
