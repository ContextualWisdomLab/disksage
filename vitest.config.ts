import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [sveltekit()],
  test: {
    include: ["src/**/*.test.ts"],
    // GitHub-hosted Ubuntu runners can expose enough logical CPUs for Vitest's
    // default forks pool to exceed the job's memory budget. Keep file isolation
    // and the complete test set; only bound concurrent worker processes in CI.
    maxWorkers: process.env.CI ? 2 : undefined,
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
        "src/lib/podmanEvidence.ts",
        "src/lib/podmanEvidenceError.ts",
        "src/lib/translationLedger.ts",
      ],
      reporter: ["text", "json", "json-summary"],
      // 위 순수 로직/API 파일은 헤드리스로 완전 검증할 수 있으므로 네 지표를 모두 100%로 고정한다.
      // 새 production 모듈은 이 denominator에 명시적으로 추가하고, 미측정 파일을 이유로 범위를 줄이지 않는다.
      thresholds: {
        statements: 100,
        branches: 100,
        functions: 100,
        lines: 100,
      },
    },
  },
});
