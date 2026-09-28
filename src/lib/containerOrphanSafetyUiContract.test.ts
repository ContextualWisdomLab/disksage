import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Container orphan cleanup safety UX", () => {
  it("never claims running services or default networks are deletion targets", () => {
    const source = readSource("src/lib/ContainerOrphanCleanup.svelte");

    expect(source).toContain("실행 중인 서비스와 기본 네트워크는 절대 건드리지 않습니다");
    expect(source).toContain("실행 중·일시정지 컨테이너는 절대 대상에 포함되지 않습니다");
    expect(source).toContain(
      "태그가 붙은 이미지는 삭제되지 않고",
    );
  });

  it("describes BuildKit cleanup as exact reviewed-ID deletion", () => {
    const source = readSource("src/lib/ContainerOrphanCleanup.svelte");

    expect(source).toContain("승인된 BuildKit 캐시 ID 집합");
    expect(source).toContain("그 항목만 정리합니다");
    expect(source).not.toContain("전체 미사용 빌드 캐시 정리");
  });

  it("gates execution behind exact phrase and non-empty rationale", () => {
    const source = readSource("src/lib/ContainerOrphanCleanup.svelte");
    const start = source.indexOf("function pruneReady(");
    const end = source.indexOf("async function inspect()", start);
    const pruneReady = source.slice(start, end);

    expect(start).toBeGreaterThanOrEqual(0);
    expect(pruneReady).toContain(".trim() === phrase");
    expect(pruneReady).toContain("(rationales[categoryKey(key, category)]?.trim().length ?? 0) > 0");
  });

  it("prevents inspect and prune from overlapping", () => {
    const source = readSource("src/lib/ContainerOrphanCleanup.svelte");
    const inspectStart = source.indexOf("async function inspect()");
    const inspectEnd = source.indexOf("async function prune(", inspectStart);
    const inspectBody = source.slice(inspectStart, inspectEnd);
    const pruneReadyStart = source.indexOf("function pruneReady(");
    const pruneReadyEnd = source.indexOf("async function inspect()", pruneReadyStart);
    const pruneReadyBody = source.slice(pruneReadyStart, pruneReadyEnd);

    expect(inspectBody).toContain("if (busy || pruneBusyKey !== null) return;");
    expect(pruneReadyBody).toContain("if (busy || phrase === null || pruneBusyKey !== null) return false;");
    expect(source).toContain("disabled={busy || pruneBusyKey !== null}");
  });

  it("requires deliberate re-entry instead of revealing the destructive approval phrase in the input", () => {
    const source = readSource("src/lib/ContainerOrphanCleanup.svelte");
    const markup = source.slice(source.indexOf("</script>"));

    expect(markup).toContain("<code>{cat.approval_phrase}</code>");
    expect(markup).toContain('placeholder="위 승인 문구를 직접 입력하세요"');
    expect(markup).not.toContain("placeholder={cat.approval_phrase}");
  });

  it("submits the exact user-typed approval phrase to the mutation boundary", () => {
    const source = readSource("src/lib/ContainerOrphanCleanup.svelte");
    const start = source.indexOf("async function prune(");
    const end = source.indexOf("</script>", start);
    const pruneBody = source.slice(start, end);

    expect(start).toBeGreaterThanOrEqual(0);
    expect(pruneBody).toContain("const typedPhrase = phrases[key]?.trim();");
    expect(pruneBody).toContain("if (!typedPhrase || typedPhrase !== cat.approval_phrase) return;");
    expect(pruneBody).toMatch(/category,\s*typedPhrase,\s*rationale,/);
    expect(pruneBody).not.toContain("const phrase = cat.approval_phrase;");
  });

  it("never recovers mutation scope from public display metadata", () => {
    const source = readSource("src/lib/ContainerOrphanCleanup.svelte");
    const start = source.indexOf("function executionScope(");
    const end = source.indexOf("function categoryKey(", start);
    const scopeContract = source.slice(start, end);

    expect(start).toBeGreaterThanOrEqual(0);
    expect(source).not.toContain("plan.runtime.display_name.split(");
    expect(scopeContract).toContain('case "docker-native": return null;');
    expect(scopeContract).toContain('case "docker-colima-context": return "colima";');
    expect(scopeContract).toContain('case "podman-machine": return "podman-machine-default";');
  });

  it("requires an explicit confirmation dialog before any irreversible prune", () => {
    const source = readSource("src/lib/ContainerOrphanCleanup.svelte");
    const start = source.indexOf("async function prune(");
    const end = source.indexOf("</script>", start);
    const pruneBody = source.slice(start, end);

    expect(pruneBody).toContain('kind: "warning"');
    expect(pruneBody).toContain("if (!granted) return;");
  });

  it("discards stale approval state after every execution attempt", () => {
    const source = readSource("src/lib/ContainerOrphanCleanup.svelte");
    const inspectStart = source.indexOf("async function inspect()");
    const inspectBody = source.slice(inspectStart, source.indexOf("}", source.indexOf("busy = false;", inspectStart)));
    expect(inspectBody).toContain("phrases = {};");
    expect(inspectBody).toContain("rationales = {};");

    // 성공·실패 모두 실행 후 계획을 폐기해 만료된 문구로 재실행되지 않게 합니다.
    expect(source).toContain("plans = await api.inspectContainerOrphans();");
    expect(source).not.toContain("executions[key] = undefined");
  });

  it("keeps a completed prune receipt visible when the post-prune refresh fails", () => {
    const source = readSource("src/lib/ContainerOrphanCleanup.svelte");
    const pruneStart = source.indexOf("async function prune(");
    const pruneBody = source.slice(pruneStart, source.indexOf("</script>", pruneStart));
    const markup = source.slice(source.indexOf("</script>"));

    expect(pruneBody).toContain("lastRefreshFailedExecution = {");
    expect(pruneBody).toContain("plans = [];");
    expect(markup).toContain("{#if lastRefreshFailedExecution}");
    expect(markup).toContain("최근 정리 결과는 보존했습니다");
    expect(markup).toContain("lastRefreshFailedExecution.execution.observed_available_gain_bytes");
  });

  it("announces failures to assistive technology with actionable copy only", () => {
    const source = readSource("src/lib/ContainerOrphanCleanup.svelte");

    expect(source).toContain('<p class="error" role="alert">');
    expect(source).toContain('aria-live="polite"');
    expect(source).toContain("containerOrphanInspectErrorMessage(error)");
    expect(source).toContain("containerOrphanPruneErrorMessage(error)");
    expect(source).not.toContain("error.slice(");
    expect(source).not.toContain("detail.slice(");
    // 내부 구현 경계 용어가 고객에게 보이는 마크업에 노출되지 않는지 확인합니다.
    const markup = source.slice(source.indexOf("</script>"));
    expect(markup).not.toContain("candidate_set_sha256");
    expect(markup).not.toContain("TOCTOU");
  });

  it("labels every form control for keyboard and screen-reader users", () => {
    const source = readSource("src/lib/ContainerOrphanCleanup.svelte");

    expect((source.match(/<label>/g) ?? []).length).toBeGreaterThanOrEqual(2);
    expect(source).toContain('aria-labelledby="container-orphan-heading"');
    expect(source).toContain("disabled={pruneBusyKey !== null}");
  });

  it("hides unavailable runtime panels while preserving an actionable summary", () => {
    const source = readSource("src/lib/ContainerOrphanCleanup.svelte");

    expect(source).toContain("plans.filter((plan) => plan.runtime.healthy)");
    expect(source).toContain("{#each healthyPlans as plan");
    expect(source).toContain("사용할 수 없는 개발 환경");
    expect(source).toContain("사용할 환경을 시작한 뒤 다시 확인하세요");
  });
});
