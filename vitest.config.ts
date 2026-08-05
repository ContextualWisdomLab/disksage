import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    coverage: {
      provider: "v8",
      // ponytail: 커버리지는 헤드리스로 검증 가능한 순수 로직과 mockable Tauri API 래퍼만 측정.
      // Svelte 컴포넌트는 별도 server-rendered behavior tests와 데스크톱 통합 검증으로 다룬다.
      include: [
        "src/lib/api.ts",
        "src/lib/treemap.ts",
        "src/lib/fmt.ts",
        "src/lib/dupeGuard.ts",
        "src/lib/verdictBadge.ts",
        "src/lib/podmanApi.ts",
        "src/lib/podmanEvidence.ts",
      ],
      reporter: ["text", "json", "json-summary"],
      // ponytail: 위 순수 로직/API 파일은 헤드리스로 완전 검증 가능하므로
      // 네 지표 모두 100%로 고정한다. Svelte 뷰는 server-rendered privacy/semantics tests와
      // Tauri package validation을 별도로 통과해야 한다.
      thresholds: {
        statements: 100,
        branches: 100,
        functions: 100,
        lines: 100,
      },
    },
  },
});
