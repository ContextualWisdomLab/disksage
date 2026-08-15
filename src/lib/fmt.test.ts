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
  it("uses the complete IEC 80000-13:2025 binary-prefix ladder", () => {
    expect(fmtBytes(1024 ** 5)).toBe("1.0 PiB");
    expect(fmtBytes(1024 ** 6)).toBe("1.0 EiB");
    expect(fmtBytes(1024 ** 7)).toBe("1.0 ZiB");
    expect(fmtBytes(1024 ** 8)).toBe("1.0 YiB");
    expect(fmtBytes(1024 ** 9)).toBe("1.0 RiB");
    expect(fmtBytes(1024 ** 10)).toBe("1.0 QiB");
  });
});
