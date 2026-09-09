/**
 * Format a byte count with IEC 80000-13:2025 binary prefixes.
 *
 * Scaled values below ten retain one decimal place. Byte values and scaled
 * values of ten or more use the existing whole-number presentation so callers
 * keep a compact, consistent UI.
 */
export function fmtBytes(n: number): string {
  const units = [
    "B",
    "KiB",
    "MiB",
    "GiB",
    "TiB",
    "PiB",
    "EiB",
    "ZiB",
    "YiB",
    "RiB",
    "QiB",
  ];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 10 || i === 0 ? 0 : 1)} ${units[i]}`;
}
