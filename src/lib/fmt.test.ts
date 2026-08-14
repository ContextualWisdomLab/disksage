import { describe, expect, it } from "vitest";
import { fmtBytes } from "./fmt";

describe("fmtBytes", () => {
  it("formats bytes without decimals", () => {
    expect(fmtBytes(0)).toBe("0 B");
    expect(fmtBytes(512)).toBe("512 B");
  });
  it("uses IEC binary prefixes when scaling by powers of 1024", () => {
    expect(fmtBytes(1024)).toBe("1.0 KiB");
    expect(fmtBytes(1536)).toBe("1.5 KiB");
    expect(fmtBytes(1024 ** 2)).toBe("1.0 MiB");
    expect(fmtBytes(1024 ** 3)).toBe("1.0 GiB");
    expect(fmtBytes(1024 ** 4)).toBe("1.0 TiB");
  });
  it("drops decimals at 10 and above", () => {
    expect(fmtBytes(10 * 1024)).toBe("10 KiB");
  });
  it("keeps scaling beyond tebibytes instead of reporting thousands of TiB", () => {
    expect(fmtBytes(1024 ** 5)).toBe("1.0 PiB");
  });
});
