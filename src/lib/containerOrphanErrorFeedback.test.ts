import { describe, expect, it } from "vitest";
import {
  containerOrphanInspectErrorMessage,
  containerOrphanPruneErrorMessage,
} from "./containerOrphanErrorFeedback";

describe("container orphan privacy-safe error feedback", () => {
  it("never reflects inspect backend details, paths, or tokens", () => {
    const secret = "runtime-info-failed:/Users/alice/.docker/token=secret-value";

    const message = containerOrphanInspectErrorMessage(secret);

    expect(message).toBe("개발 환경 확인 실패 — 잠시 후 다시 시도해 주세요.");
    expect(message).not.toContain("alice");
    expect(message).not.toContain("secret-value");
    expect(message).not.toContain("runtime-info");
    expect(message).not.toMatch(/런타임|증거|fingerprint|stderr/i);
  });

  it("maps only approved prune boundary codes and never reflects arbitrary detail", () => {
    expect(
      containerOrphanPruneErrorMessage(
        "orphan-prune-confirmation-mismatch:/Users/alice/private",
      ),
    ).toBe("승인 문구가 최신 목록과 일치하지 않습니다. 새로 확인한 뒤 문구를 다시 입력해 주세요.");
    expect(containerOrphanPruneErrorMessage("orphan-prune-empty-candidate-set:anything")).toBe(
      "삭제 대상이 사라졌습니다. 다시 확인해 주세요.",
    );
    expect(containerOrphanPruneErrorMessage("orphan-prune-evidence-incomplete:socket detail")).toBe(
      "확인이 끝나지 않아 실행이 중단되었습니다. 개발 환경 상태를 확인한 뒤 다시 시도해 주세요.",
    );

    const unknown = containerOrphanPruneErrorMessage(
      "runtime-delete-failed:/Users/alice/.docker/config.json token=secret-value",
    );
    expect(unknown).toBe("정리 실행 실패 — 데이터는 그대로입니다. 상태를 확인한 뒤 다시 시도해 주세요.");
    expect(unknown).not.toContain("alice");
    expect(unknown).not.toContain("secret-value");
    expect(unknown).not.toContain("runtime-delete-failed");
    expect(unknown).not.toMatch(/런타임|증거|fingerprint|stderr/i);
  });

  it("treats non-string errors as opaque", () => {
    expect(containerOrphanInspectErrorMessage(new Error("/private/path"))).toBe(
      "개발 환경 확인 실패 — 잠시 후 다시 시도해 주세요.",
    );
    expect(containerOrphanPruneErrorMessage({ detail: "token=secret" })).toBe(
      "정리 실행 실패 — 데이터는 그대로입니다. 상태를 확인한 뒤 다시 시도해 주세요.",
    );
  });
});
