import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./ui/design-tokens.css", import.meta.url), "utf8");

function tokenHex(name: string): string {
  const declaration = css
    .split(/\r?\n/)
    .find((line) => line.trimStart().startsWith(`--${name}:`));
  const match = declaration?.match(/#[0-9a-fA-F]{6}/);
  if (!match) throw new Error(`missing color token: ${name}`);
  return match[0];
}

function channelLuminance(channel: number): number {
  const normalized = channel / 255;
  return normalized <= 0.04045
    ? normalized / 12.92
    : ((normalized + 0.055) / 1.055) ** 2.4;
}

function relativeLuminance(hex: string): number {
  const channels = [1, 3, 5].map((offset) =>
    Number.parseInt(hex.slice(offset, offset + 2), 16),
  );
  return (
    0.2126 * channelLuminance(channels[0]) +
    0.7152 * channelLuminance(channels[1]) +
    0.0722 * channelLuminance(channels[2])
  );
}

function contrastRatio(first: string, second: string): number {
  const firstLuminance = relativeLuminance(first);
  const secondLuminance = relativeLuminance(second);
  const lighter = Math.max(firstLuminance, secondLuminance);
  const darker = Math.min(firstLuminance, secondLuminance);
  return (lighter + 0.05) / (darker + 0.05);
}

describe("design-token accessibility contracts", () => {
  it("keeps the keyboard skip link at WCAG AA text contrast in every color scheme", () => {
    const rule = css.match(/\.ds-skip-link\s*\{([\s\S]*?)\}/)?.[1];
    expect(rule).toBeTruthy();

    const backgroundToken = rule?.match(/background:\s*var\(--([^)]+)\)/)?.[1];
    const foregroundToken = rule?.match(/color:\s*var\(--([^)]+)\)/)?.[1];
    expect(backgroundToken).toBe("ds-blue-700");
    expect(foregroundToken).toBe("ds-white");

    expect(
      contrastRatio(tokenHex(backgroundToken!), tokenHex(foregroundToken!)),
    ).toBeGreaterThanOrEqual(4.5);
  });
});
