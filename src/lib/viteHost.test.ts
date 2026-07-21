import { describe, expect, it } from "vitest";

import { resolveDevHost } from "../../vite.config.js";

describe("resolveDevHost", () => {
  it("keeps the default development server loopback-only", () => {
    expect(resolveDevHost(undefined)).toBeUndefined();
    expect(resolveDevHost("   ")).toBeUndefined();
  });

  it.each([
    "0.0.0.0",
    "0",
    "0x0",
    "000.000.000.000",
    "::",
    "::0",
    "[::]",
    "0:0::0",
    "0:0:0:0:0:0:0:0",
    "[0:0:0:0:0:0:0:0]",
    "*",
  ])("rejects wildcard bind form %s", (value) => {
    expect(() => resolveDevHost(value)).toThrow(/must not bind all network interfaces/);
  });

  it.each(["localhost", "127.0.0.1", "::1", "192.0.2.10", "device.local"])(
    "preserves explicit non-wildcard host %s",
    (value) => {
      expect(resolveDevHost(value)).toBe(value);
    },
  );

  it.each([
    "user@example.test",
    "example.test:1420",
    "example.test/path",
    "//example.test",
    "example.test\\path",
  ])(
    "rejects non-host syntax %s",
    (value) => {
      expect(() => resolveDevHost(value)).toThrow(/must contain only a host name or IP address/);
    },
  );
});
