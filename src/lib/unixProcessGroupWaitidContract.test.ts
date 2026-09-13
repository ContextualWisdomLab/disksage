import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(
  new URL("../../src-tauri/src/unix_process_group.rs", import.meta.url),
  "utf8",
);

describe("Unix waitid no-reap observation contract", () => {
  it("uses the returned child PID, not si_signo, to distinguish WNOHANG from an exited child", () => {
    expect(source).toContain("info.si_pid()");
    expect(source).not.toContain("match info.si_signo");
  });
});
