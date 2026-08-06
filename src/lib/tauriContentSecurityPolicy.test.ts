import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

interface TauriSecurityConfig {
  app: {
    security: {
      csp: null | Record<string, string | string[]>;
    };
  };
}

/** Read and parse the source-controlled Tauri configuration under test. */
function readTauriSecurityConfig(): TauriSecurityConfig {
  return JSON.parse(
    readFileSync(resolve(repositoryRoot, "src-tauri/tauri.conf.json"), "utf8"),
  ) as TauriSecurityConfig;
}

describe("Tauri content security policy", () => {
  it("fails closed to bundled content plus the Tauri IPC transport", () => {
    const csp = readTauriSecurityConfig().app.security.csp;

    expect(csp).toEqual({
      "default-src": "'self'",
      "base-uri": "'none'",
      "connect-src": "ipc: http://ipc.localhost",
      "font-src": "'self'",
      "frame-src": "'none'",
      "img-src": "'self' data: blob:",
      "object-src": "'none'",
      "script-src": "'self'",
      "style-src": "'self' 'unsafe-inline'",
    });
  });

  it("does not authorize wildcard, remote script, remote style, or eval sources", () => {
    const csp = readTauriSecurityConfig().app.security.csp;
    expect(csp).not.toBeNull();

    const serialized = JSON.stringify(csp);
    expect(serialized).not.toContain("*");
    expect(serialized).not.toContain("https://");
    expect(serialized).not.toContain("'unsafe-eval'");
    expect(serialized).not.toContain("'wasm-unsafe-eval'");
    expect(csp?.["script-src"]).not.toContain("'unsafe-inline'");
    expect(csp?.["connect-src"]).toBe("ipc: http://ipc.localhost");
  });
});
