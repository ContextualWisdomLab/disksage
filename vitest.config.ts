import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    coverage: {
      provider: "v8",
      // ponytail: 커버리지는 헤드리스로 검증 가능한 순수 로직과 mockable Tauri API 래퍼만 측정.
      // Svelte 컴포넌트는 GUI·통합 검증 영역 (cargo test + 수동 체크리스트)
      include: [
        "src/lib/api.ts",
        "src/lib/treemap.ts",
        "src/lib/fmt.ts",
        "src/lib/dupeGuard.ts",
        "src/lib/verdictBadge.ts",
      ],
      reporter: ["text", "json", "json-summary"],
    },
  },
});
