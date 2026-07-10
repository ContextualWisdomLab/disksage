import { describe, expect, it } from "vitest";
import { fmtBytes } from "./fmt";

describe("fmtBytes", () => {
  it("formats bytes without decimals", () => {
    expect(fmtBytes(0)).toBe("0 B");
    expect(fmtBytes(512)).toBe("512 B");
  });
  it("uses one decimal below 10 in larger units", () => {
    expect(fmtBytes(1024)).toBe("1.0 KB");
    expect(fmtBytes(1536)).toBe("1.5 KB");
  });
  it("drops decimals at 10 and above", () => {
    expect(fmtBytes(10 * 1024)).toBe("10 KB");
  });
  it("caps at TB", () => {
    expect(fmtBytes(1024 ** 5)).toBe("1024 TB");
  });
});
