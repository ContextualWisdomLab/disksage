import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const config = readFileSync(new URL("../../vitest.config.ts", import.meta.url), "utf8");

describe("frontend production coverage scope", () => {
  it("measures every source-controlled production TypeScript module", () => {
    expect(config).toContain('include: ["src/lib/**/*.ts", "src/routes/**/*.ts"]');
    expect(config).toContain('exclude: ["**/*.test.ts", "**/*.d.ts"]');
    for (const legacyAllowlistEntry of [
      "src/lib/api.ts",
      "src/lib/treemap.ts",
      "src/lib/fmt.ts",
      "src/lib/dupeGuard.ts",
      "src/lib/verdictBadge.ts",
    ]) {
      expect(config).not.toContain(`        "${legacyAllowlistEntry}",`);
    }
  });
});
