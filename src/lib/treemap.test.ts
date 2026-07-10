import { describe, expect, it } from "vitest";
import { squarify } from "./treemap";

const area = (r: { w: number; h: number }) => r.w * r.h;

describe("squarify", () => {
  it("fills the container, areas proportional to values", () => {
    const rects = squarify(
      [
        { key: "a", value: 6 },
        { key: "b", value: 3 },
        { key: "c", value: 1 },
      ],
      0, 0, 100, 100,
    );
    expect(rects).toHaveLength(3);
    expect(rects.reduce((s, r) => s + area(r), 0)).toBeCloseTo(10000, 3);
    expect(area(rects.find((r) => r.key === "a")!)).toBeCloseTo(6000, 3);
  });

  it("keeps every rect inside the container", () => {
    const items = Array.from({ length: 20 }, (_, i) => ({ key: String(i), value: i + 1 }));
    for (const r of squarify(items, 0, 0, 300, 200)) {
      expect(r.x).toBeGreaterThanOrEqual(-1e-6);
      expect(r.y).toBeGreaterThanOrEqual(-1e-6);
      expect(r.x + r.w).toBeLessThanOrEqual(300 + 1e-6);
      expect(r.y + r.h).toBeLessThanOrEqual(200 + 1e-6);
    }
  });

  it("drops zero/negative values and handles empty input", () => {
    expect(squarify([{ key: "z", value: 0 }, { key: "n", value: -5 }], 0, 0, 10, 10)).toEqual([]);
    expect(squarify([], 0, 0, 10, 10)).toEqual([]);
  });
});
