import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

type CspPolicy = null | Record<string, string | string[]>;

interface TauriSecurityConfig {
  app: {
    security: {
      csp: CspPolicy;
      devCsp?: CspPolicy;
    };
  };
}

/** Read one UTF-8 file from the source-controlled repository root. */
function readRepositoryFile(relativePath: string): string {
  return readFileSync(resolve(repositoryRoot, relativePath), "utf8");
}

/** Read and parse the source-controlled Tauri configuration under test. */
function readTauriSecurityConfig(): TauriSecurityConfig {
  return JSON.parse(
    readRepositoryFile("src-tauri/tauri.conf.json"),
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
      "form-action": "'none'",
      "frame-src": "'none'",
      "img-src": "'self' data: blob:",
      "manifest-src": "'none'",
      "media-src": "'none'",
      "object-src": "'none'",
      "script-src": "'self'",
      "style-src": "'self' 'unsafe-inline'",
      "worker-src": "'none'",
    });
  });

  it("keeps development HMR functional without widening the production network boundary", () => {
    const security = readTauriSecurityConfig().app.security;

    expect(security.devCsp).toEqual({
      "default-src": "'self'",
      "base-uri": "'none'",
      "connect-src": "'self' ipc: http://ipc.localhost ws:",
      "font-src": "'self'",
      "form-action": "'none'",
      "frame-src": "'none'",
      "img-src": "'self' data: blob:",
      "manifest-src": "'none'",
      "media-src": "'none'",
      "object-src": "'none'",
      "script-src": "'self'",
      "style-src": "'self' 'unsafe-inline'",
      "worker-src": "'none'",
    });
    expect(security.csp?.["connect-src"]).toBe("ipc: http://ipc.localhost");
    expect(security.csp?.["connect-src"]).not.toContain("ws:");
  });

  it("denies unused worker, media, and manifest fetch authority explicitly", () => {
    const security = readTauriSecurityConfig().app.security;

    for (const policy of [security.csp, security.devCsp]) {
      expect(policy?.["worker-src"]).toBe("'none'");
      expect(policy?.["media-src"]).toBe("'none'");
      expect(policy?.["manifest-src"]).toBe("'none'");
    }
  });

  it("blocks form submissions because form-action does not fall back to default-src", () => {
    const security = readTauriSecurityConfig().app.security;

    expect(security.csp?.["form-action"]).toBe("'none'");
    expect(security.devCsp?.["form-action"]).toBe("'none'");
  });

  it("does not authorize wildcard, remote script, remote style, or eval sources", () => {
    const security = readTauriSecurityConfig().app.security;
    expect(security.csp).not.toBeNull();
    expect(security.devCsp).not.toBeNull();

    for (const policy of [security.csp, security.devCsp]) {
      const serialized = JSON.stringify(policy);
      expect(serialized).not.toContain("*");
      expect(serialized).not.toContain("https://");
      expect(serialized).not.toContain("'unsafe-eval'");
      expect(serialized).not.toContain("'wasm-unsafe-eval'");
      expect(policy?.["script-src"]).not.toContain("'unsafe-inline'");
    }
    expect(security.csp?.["connect-src"]).toBe("ipc: http://ipc.localhost");
  });

  it("keeps the security decision, rollback procedure, standards, and changelog durable", () => {
    const doctoring = readRepositoryFile(
      "docs/doctoring/tauri-content-security-policy.md",
    );
    const changelog = readRepositoryFile("CHANGELOG.md");

    expect(doctoring).toContain("# Tauri content security policy");
    expect(doctoring).toContain("form-action 'none'");
    expect(doctoring).toContain("does not fall back to `default-src`");
    expect(doctoring).toContain(
      "`worker-src 'none'`, `media-src 'none'`, and `manifest-src 'none'`",
    );
    expect(doctoring).toContain("## Development policy");
    expect(doctoring).toContain("## Rollback and migration");
    expect(doctoring).toContain("## Standalone and MSA compatibility");
    expect(doctoring).toContain("## APA 7th references");
    expect(doctoring).toContain("Content Security Policy Level 2");
    expect(doctoring).toContain("Content Security Policy Level 3");
    expect(doctoring).toContain("https://v2.tauri.app/security/csp/");
    expect(doctoring).toContain("https://vite.dev/config/server-options");
    expect(doctoring).toContain(
      "World Wide Web Consortium. (2026, July 29). *Content Security Policy Level 3*",
    );
    expect(doctoring).toContain(
      "https://www.w3.org/TR/2026/WD-CSP3-20260729/",
    );
    expect(doctoring).not.toContain("WD-CSP3-20260505");
    expect(changelog).toContain("deny form submissions");
    expect(changelog).toContain(
      "deny unused worker, media, and web-app-manifest fetch authority",
    );
  });
});
