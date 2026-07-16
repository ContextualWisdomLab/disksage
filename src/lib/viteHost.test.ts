import { describe, expect, it } from "vitest";

import { safeTauriDevHost } from "./viteHost";

describe("safeTauriDevHost", () => {
  it("keeps unset and unsafe listener addresses on Vite's loopback default", () => {
    for (const value of [
      undefined,
      "",
      " ",
      "0.0.0.0",
      "::",
      "192.168.1.10",
      "dev.example.com",
    ]) {
      expect(safeTauriDevHost(value)).toBe(false);
    }
  });

  it("accepts only normalized loopback host values", () => {
    expect(safeTauriDevHost("localhost")).toBe("localhost");
    expect(safeTauriDevHost(" LOCALHOST ")).toBe("localhost");
    expect(safeTauriDevHost("127.0.0.1")).toBe("127.0.0.1");
    expect(safeTauriDevHost("::1")).toBe("::1");
    expect(safeTauriDevHost("[::1]")).toBe("[::1]");
  });
});
