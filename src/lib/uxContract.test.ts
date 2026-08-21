import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = resolve(import.meta.dirname, "../..");
const read = (relativePath: string) => readFileSync(resolve(root, relativePath), "utf8");

describe("UI/UX design and Storybook contract", () => {
  it("keeps primitive, semantic, and component tokens with preference fallbacks", () => {
    const tokens = read("src/lib/ui/design-tokens.css");
    expect(tokens).toContain("--ds-blue-700");
    expect(tokens).toContain("--ds-text: var(--ds-slate-950)");
    expect(tokens).toContain("--ds-control-min-size: 2.75rem");
    expect(tokens).toContain("prefers-color-scheme: dark");
    expect(tokens).toContain("prefers-reduced-motion: reduce");
    expect(tokens).toContain("forced-colors: active");
  });

  it("keeps the shell keyboard and live-feedback boundaries explicit", () => {
    const layout = read("src/routes/+layout.svelte");
    const page = read("src/routes/+page.svelte");
    expect(layout).toContain('href="#main-content"');
    expect(page).toContain('id="main-content" tabindex="-1"');
    expect(page).toContain('for="scan-root"');
    expect(page).toContain('role="alert"');
    expect(page).toContain('role="group" aria-label="스캔 제어"');
    expect(page).toContain('aria-live="polite"');
    expect(page).not.toContain("alert(`스캔 시작 실패");
  });

  it("registers every provider state and interaction edge in Storybook", () => {
    const story = read("src/lib/ux/ProviderStatusCard.stories.ts");
    const config = read(".storybook/preview.ts");
    const workflow = read(".github/workflows/test.yml");
    for (const state of ["clear", "checking", "provider-sync-incomplete", "materialization-stalled"]) {
      expect(story).toContain(`state: "${state}"`);
    }
    expect(story).toContain("toHaveBeenCalledOnce");
    expect(story).toContain("toBeDisabled");
    expect(config).toContain('test: "error"');
    expect(config).toContain("mobile");
    expect(config).toContain('defaultViewport: "desktop"');
    expect(story).toContain('defaultViewport: "mobile"');
    expect(workflow).toContain("npm run build-storybook");
    expect(workflow).toContain("playwright install --with-deps chromium");
    expect(workflow).toContain("python3 -m http.server 6006 --directory storybook-static");
    expect(workflow).toContain("npm run test-storybook");
    expect(read(".storybook/test-runner.ts")).toContain("setViewportSize");
  });

  it("uses release-consumer terminology rather than a shopping-domain actor", () => {
    const files = [
      "CHANGELOG.md",
      "docs/product-technical-gap-baseline.md",
      "docs/doctoring/release-version-contract.md",
      "docs/doctoring/release-artifact-provenance.md",
      "scripts/ci/release-version.mjs",
    ];
    for (const file of files) expect(read(file).toLowerCase()).not.toMatch(/\bbuyer\b/);
  });
});
