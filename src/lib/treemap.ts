export interface TreemapItem {
  key: string;
  value: number;
}
export interface TreemapRect extends TreemapItem {
  x: number;
  y: number;
  w: number;
  h: number;
}

// Squarified treemap (Bruls, Huizing, van Wijk). value <= 0 항목은 제외.
export function squarify(
  items: TreemapItem[],
  x0: number,
  y0: number,
  w0: number,
  h0: number,
): TreemapRect[] {
  const src = items.filter((i) => i.value > 0).sort((a, b) => b.value - a.value);
  const total = src.reduce((s, i) => s + i.value, 0);
  const out: TreemapRect[] = [];
  if (total === 0 || w0 <= 0 || h0 <= 0) return out;

  const scale = (w0 * h0) / total;
  let x = x0, y = y0, w = w0, h = h0;
  type Scaled = { key: string; value: number; area: number };
  let row: Scaled[] = [];

  const rowSum = (r: Scaled[]) => r.reduce((s, i) => s + i.area, 0);
  const worst = (r: Scaled[], side: number) => {
    const s = rowSum(r);
    const s2 = s * s;
    const side2 = side * side;
    let max = -Infinity, min = Infinity;
    for (const i of r) {
      if (i.area > max) max = i.area;
      if (i.area < min) min = i.area;
    }
    return Math.max((side2 * max) / s2, s2 / (side2 * min));
  };
  const layoutRow = (r: Scaled[]) => {
    const s = rowSum(r);
    if (w >= h) {
      const thick = s / h;
      let yy = y;
      for (const i of r) {
        const hh = i.area / thick;
        out.push({ key: i.key, value: i.value, x, y: yy, w: thick, h: hh });
        yy += hh;
      }
      x += thick;
      w -= thick;
    } else {
      const thick = s / w;
      let xx = x;
      for (const i of r) {
        const ww = i.area / thick;
        out.push({ key: i.key, value: i.value, x: xx, y, w: ww, h: thick });
        xx += ww;
      }
      y += thick;
      h -= thick;
    }
  };

  for (const it of src) {
    const item: Scaled = { key: it.key, value: it.value, area: it.value * scale };
    const side = Math.min(w, h);
    if (row.length === 0 || worst([...row, item], side) <= worst(row, side)) {
      row.push(item);
    } else {
      layoutRow(row);
      row = [item];
    }
  }
  layoutRow(row); // 빈 입력은 위에서 early-return — 여기선 row가 항상 비어있지 않음
  return out;
}
