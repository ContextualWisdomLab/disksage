import { describe, expect, it } from "vitest";

import { resolveDevHost } from "../../vite.config.js";

describe("resolveDevHost", () => {
  it("keeps the default development server loopback-only", async () => {
    await expect(resolveDevHost(undefined)).resolves.toBeUndefined();
    await expect(resolveDevHost("   ")).resolves.toBeUndefined();
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
    return expect(resolveDevHost(value)).rejects.toThrow(/must not bind all network interfaces/);
  });

  it.each([
    ["127.0.0.1", "127.0.0.1"],
    ["0x7f000001", "127.0.0.1"],
    ["::1", "::1"],
    ["[::1]", "::1"],
    ["192.0.2.10", "192.0.2.10"],
  ])("canonicalizes explicit non-wildcard address %s", async (value, expected) => {
    await expect(resolveDevHost(value)).resolves.toBe(expected);
  });

  it.each([
    ["localhost", [{ address: "127.0.0.1", family: 4 }], "127.0.0.1"],
    ["device.local", [{ address: "192.0.2.10", family: 4 }], "192.0.2.10"],
    ["ipv6.local", [{ address: "2001:db8::10", family: 6 }], "2001:db8::10"],
  ])("binds host name %s to its validated numeric address", async (value, answers, expected) => {
    const resolveAll = async (hostname: string) => {
      expect(hostname).toBe(value);
      return answers;
    };
    await expect(resolveDevHost(value, resolveAll)).resolves.toBe(expected);
  });

  it.each([
    ["wildcard.0.0.0.0.nip.io", [{ address: "0.0.0.0", family: 4 }]],
    ["ipv6-wildcard.test", [{ address: "::", family: 6 }]],
    ["mapped-wildcard.test", [{ address: "::ffff:0.0.0.0", family: 6 }]],
    [
      "mixed.test",
      [
        { address: "192.0.2.10", family: 4 },
        { address: "0.0.0.0", family: 4 },
      ],
    ],
  ])("rejects host name %s when any DNS result is a wildcard", async (value, answers) => {
    await expect(resolveDevHost(value, async () => answers)).rejects.toThrow(
      /must not bind all network interfaces/,
    );
  });

  it("rejects unresolved and empty DNS results", async () => {
    await expect(
      resolveDevHost("missing.test", async () => {
        throw new Error("not found");
      }),
    ).rejects.toThrow(/must resolve to a usable IP address/);
    await expect(resolveDevHost("empty.test", async () => [])).rejects.toThrow(
      /must resolve to a usable IP address/,
    );
  });

  it.each(["::ffff:0:0", "::ffff:0.0.0.0", "0:0:0:0:0:ffff:0:0"])(
    "rejects mapped wildcard address %s",
    async (value) => {
      await expect(resolveDevHost(value)).rejects.toThrow(/must not bind all network interfaces/);
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
    async (value) => {
      await expect(resolveDevHost(value)).rejects.toThrow(
        /must contain only a host name or IP address/,
      );
    },
  );
});
