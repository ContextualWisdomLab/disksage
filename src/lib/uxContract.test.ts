import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = resolve(import.meta.dirname, "../..");
const read = (relativePath: string) => readFileSync(resolve(root, relativePath), "utf8");

type CssRule = { selector: string; body: string };

const cssRules = (css: string): CssRule[] =>
  [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((match) => ({
    selector: match[1].trim(),
    body: match[2],
  }));

const ruleHasBareControlSelector = ({ selector }: CssRule) =>
  selector
    .split(",")
    .map((part) => part.trim())
    .some((part) => /^(?:button|select|input|textarea)$/.test(part));

describe("UI/UX design and Storybook contract", () => {
  it("keeps primitive, semantic, and component tokens with preference fallbacks", () => {
    const tokens = read("src/lib/ui/design-tokens.css");
    expect(tokens).toContain("--ds-blue-700");
    expect(tokens).toContain("--ds-text: var(--ds-slate-950)");
    expect(tokens).toContain("--ds-control-min-size: 2.75rem");
    expect(tokens).toContain("prefers-color-scheme: dark");
    expect(tokens).toContain(".receipt-reconciliation");
    expect(tokens).toContain(".approval-controls");
    expect(tokens).toContain("prefers-reduced-motion: reduce");
    expect(tokens).toContain("forced-colors: active");
  });

  it("keeps visual control styling opt-in instead of restyling every legacy button", () => {
    const tokens = read("src/lib/ui/design-tokens.css");
    const providerStatus = read("src/lib/ux/ProviderStatusCard.svelte");
    const bareControlRules = cssRules(tokens).filter(ruleHasBareControlSelector);

    // Keep at least the typography normalization rule in scope so this assertion
    // cannot pass merely because the selector parser matched nothing.
    expect(bareControlRules.length).toBeGreaterThan(0);
    for (const { body } of bareControlRules) {
      expect(body).not.toMatch(/\b(?:min-height|border|background|padding)\s*:/);
    }

    expect(tokens).toMatch(/\.ds-control\s*\{[\s\S]*?min-height:\s*var\(--ds-control-min-size\)/);
    expect(tokens).toMatch(/\.ds-control\s*\{[\s\S]*?border:\s*1px solid var\(--ds-border\)/);
    expect(tokens).toMatch(/\.ds-control:hover:not\(:disabled\)/);
    expect(providerStatus).toContain('class="ds-control"');
    expect(providerStatus).toContain("h1, h2 { margin: 0; font-size: 1.1rem; }");
  });

  it("keeps form labels and notices readable in dark mode", () => {
    for (const relativePath of [
      "src/lib/BrewCleanup.svelte",
      "src/lib/GitWorktreeCleanup.svelte",
      "src/lib/Cleanup.svelte",
      "src/lib/OrphanCleanup.svelte",
    ]) {
      const source = read(relativePath);
      expect(source).not.toMatch(/(?:label|\.notice)[^{}]*color:\s*#(?:555|4d5660)\b/);
    }
  });

  it("rejects a bare-control sizing regression instead of passing vacuously", () => {
    const unsafeCss = `
button,
select,
input,
textarea {
  font: inherit;
}

button,
select,
input,
textarea {
  min-height: var(--ds-control-min-size);
}
`;
    const unsafeBareRules = cssRules(unsafeCss).filter(ruleHasBareControlSelector);

    expect(unsafeBareRules.length).toBe(2);
    expect(unsafeBareRules.some(({ body }) => /\bmin-height\s*:/.test(body))).toBe(true);
  });

  it("keeps the Storybook-owned shell skip-link boundary explicit and focusable", () => {
    const layout = read("src/routes/+layout.svelte");
    const page = read("src/routes/+page.svelte");
    expect(layout).toContain('class="ds-skip-link"');
    expect(layout).toContain('href="#main-content"');
    expect(page).toMatch(/<main\b[^>]*\bid="main-content"[^>]*\btabindex="-1"[^>]*>/);
    expect(layout).toContain('import "$lib/ui/design-tokens.css"');
  });

  it("registers every provider state and interaction edge in Storybook", () => {
    const story = read("src/lib/ux/ProviderStatusCard.stories.ts");
    const config = read(".storybook/preview.ts");
    const workflow = read(".github/workflows/storybook-accessibility.yml");
    for (const state of ["clear", "checking", "provider-sync-incomplete", "materialization-stalled"]) {
      expect(story).toContain(`state: "${state}"`);
    }
    expect(story).toContain("toHaveBeenCalledOnce");
    expect(story).toContain("toBeDisabled");
    expect(config).toContain('test: "error"');
    expect(config).toContain("mobile");
    expect(config).toContain('viewport: { value: "desktop", isRotated: false }');
    expect(story).toContain('viewport: { value: "mobile", isRotated: false }');
    expect(workflow).toContain("npm run build-storybook");
    expect(workflow).toContain("playwright install --with-deps chromium");
    expect(workflow).toContain("python3 -m http.server 6006 --directory storybook-static");
    expect(workflow).toContain("npm run test-storybook");
    expect(read(".storybook/test-runner.ts")).toContain("setViewportSize");
  });

  it("keeps Storybook customer copy actionable and free of implementation terms", () => {
    const customerSources = [
      read("src/lib/ux/ProviderStatusCard.svelte"),
      read("src/lib/ux/ProviderStatusCard.stories.ts"),
    ];
    const customerCopy = customerSources
      .flatMap((source) => [...source.matchAll(/["'`]([^"'`]*[가-힣][^"'`]*)["'`]/g)])
      .map((match) => match[1])
      .join("\n");

    expect(customerCopy).toContain("상태를 다시 확인하세요");
    expect(customerCopy).toContain("원본을 정리하지 않습니다");
    expect(customerCopy).toContain("Finder 복사 취소");
    for (const internalTerm of [
      "전역 동기화",
      "File Provider",
      "materialization",
      "staged item",
      "admission",
      "attestation",
    ]) {
      expect(customerCopy).not.toContain(internalTerm);
    }
  });
});
