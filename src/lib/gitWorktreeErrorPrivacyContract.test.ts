import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  GIT_WORKTREE_AUDIT_FAILURE,
  GIT_WORKTREE_CONFIRMATION_FAILURE,
  GIT_WORKTREE_REMOVAL_FAILURE,
  GIT_WORKTREE_REPOSITORY_SELECTION_FAILURE,
  GIT_WORKTREE_RESULT_RECORD_FAILURE,
  evidenceGapActions,
  removalStoppedAction,
} from "./gitWorktreeFeedback";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("Git worktree privacy-safe failure feedback", () => {
  it("never renders arbitrary thrown or record-persistence exception text", () => {
    const source = readSource("src/lib/GitWorktreeCleanup.svelte");

    expect(source).not.toContain("String(e)");
    expect(source).not.toContain("catch (e)");
    expect(source).not.toContain("{removal.result_record_error}");
    expect(source).toContain("GIT_WORKTREE_REPOSITORY_SELECTION_FAILURE");
    expect(source).toContain("GIT_WORKTREE_AUDIT_FAILURE");
    expect(source).toContain("GIT_WORKTREE_REMOVAL_FAILURE");
    expect(source).toContain("GIT_WORKTREE_RESULT_RECORD_FAILURE");
  });

  it("does not disclose immutable record paths in the desktop UI", () => {
    const source = readSource("src/lib/GitWorktreeCleanup.svelte");

    expect(source).not.toContain("승인 기록: {removal.approval_path}");
    expect(source).not.toContain("결과 기록: {removal.result_path}");
    expect(source).toContain("승인 기록을 DiskSage 데이터 폴더에 저장했습니다.");
    expect(source).toContain("결과 기록을 DiskSage 데이터 폴더에 저장했습니다.");
    expect(source).toContain("{#if removal.result_path}");
    expect(source).toContain("GIT_WORKTREE_RESULT_RECORD_FAILURE");
  });

  it("uses path-free failure copy that directs the next safe action", () => {
    expect(GIT_WORKTREE_REPOSITORY_SELECTION_FAILURE).toBe(
      "Git 저장소를 선택하지 못했습니다. 폴더 접근 권한과 저장소 위치를 확인한 뒤 다시 선택하세요.",
    );
    expect(GIT_WORKTREE_CONFIRMATION_FAILURE).toBe(
      "제거 확인 창을 열지 못했습니다. 다른 확인 창을 닫은 뒤 새 감사부터 다시 진행하세요.",
    );
    expect(GIT_WORKTREE_AUDIT_FAILURE).toBe(
      "Git worktree 감사를 완료하지 못했습니다. 저장소 경로와 보존할 ref가 현재 로컬에서 해석되는지 확인한 뒤 다시 감사하세요.",
    );
    expect(GIT_WORKTREE_REMOVAL_FAILURE).toBe(
      "Git worktree 제거에 실패했습니다. 저장소 상태와 계획 지문을 다시 확인한 뒤 새 감사부터 진행하세요.",
    );
    expect(GIT_WORKTREE_RESULT_RECORD_FAILURE).toBe(
      "제거 결과는 위와 같지만 기록을 저장하지 못했습니다. DiskSage 데이터 폴더의 권한과 여유 공간을 확인하고 이 화면의 결과를 별도로 보관하세요.",
    );

    for (const message of [
      GIT_WORKTREE_REPOSITORY_SELECTION_FAILURE,
      GIT_WORKTREE_CONFIRMATION_FAILURE,
      GIT_WORKTREE_AUDIT_FAILURE,
      GIT_WORKTREE_REMOVAL_FAILURE,
      GIT_WORKTREE_RESULT_RECORD_FAILURE,
    ]) {
      expect(message).not.toMatch(/(?:\/Users\/|[A-Za-z]:\\|file:\/\/)/);
      expect(message).toMatch(/(?:확인|보관|선택|감사)/);
    }
  });

  it("serializes dialogs and discards superseded async results", () => {
    const source = readSource("src/lib/GitWorktreeCleanup.svelte");

    expect(source).toContain("const seq = ++selectionSeq;");
    expect(source).toContain("if (seq !== selectionSeq || typeof selected !== \"string\") return;");
    expect(source).toContain("const seq = ++auditSeq;");
    expect(source).toContain("if (seq !== auditSeq) return;");
    expect(source).toContain("const seq = ++removalSeq;");
    expect(source).toContain("if (!approved || seq !== removalSeq || report !== approvedReport) return;");
    expect(source).toContain("disabled={choosing || planning || confirming || executing}");
    expect(source).toContain("report = null;");
  });

  it("turns every current evidence gap into bounded customer guidance", () => {
    const cases: Array<[string, string]> = [
      [
        "worktree-path-evidence-incomplete",
        "worktree 경로를 안전하게 확인하지 못했습니다. Git 등록 상태와 디렉터리 접근 권한을 확인한 뒤 다시 감사하세요.",
      ],
      [
        "git-status-evidence-incomplete",
        "변경 사항 여부를 확인하지 못했습니다. 해당 worktree에서 Git 상태를 확인한 뒤 다시 감사하세요.",
      ],
      [
        "reference-containment-evidence-incomplete",
        "HEAD가 보존 ref에 포함되는지 확인하지 못했습니다. ref를 fetch하고 정확한 ref 이름을 입력한 뒤 다시 감사하세요.",
      ],
      [
        "actor-cwd-evidence-incomplete",
        "현재 디렉터리 사용 여부를 확인하지 못했습니다. DiskSage와 터미널의 현재 디렉터리를 worktree 밖으로 옮긴 뒤 다시 감사하세요.",
      ],
      [
        "size-evidence-incomplete",
        "worktree 크기를 끝까지 측정하지 못했습니다. 접근 권한과 디스크 상태를 확인한 뒤 다시 감사하세요.",
      ],
      [
        "active-use-evidence-incomplete",
        "열린 파일과 프로세스 사용 여부를 확인하지 못했습니다. 관련 앱과 터미널을 닫은 뒤 다시 감사하세요.",
      ],
    ];

    for (const [code, action] of cases) {
      expect(evidenceGapActions([code])).toEqual([action]);
    }
  });

  it("deduplicates evidence guidance and never reflects unknown blocker text", () => {
    const fallback =
      "증거가 불완전합니다. 해당 worktree의 Git 상태와 활성 사용을 직접 확인한 뒤 다시 감사하세요.";
    const injected = "/Users/example/private-worktree: probe failed";
    expect(evidenceGapActions([injected])).toEqual([fallback]);
    expect(evidenceGapActions([injected]).join(" ")).not.toContain(injected);
    expect(evidenceGapActions([])).toEqual([fallback]);
    expect(
      evidenceGapActions(["active-use-evidence-incomplete", "active-use-evidence-incomplete"]),
    ).toHaveLength(1);
    expect(evidenceGapActions(["toString", "constructor"])).toEqual([fallback]);
  });

  it("turns removal stop reasons into bounded stop-and-recheck actions", () => {
    expect(removalStoppedAction("git-worktree-removal-live-reaudit-failed")).toBe(
      "실행 직전 재감사에 실패했습니다. 저장소 상태를 확인한 뒤 새 감사부터 진행하세요.",
    );
    expect(removalStoppedAction("git-worktree-removal-reference-drift")).toBe(
      "보존 ref가 승인 이후 변경되었습니다. 최신 ref로 새 감사를 실행하고 다시 승인하세요.",
    );
    expect(removalStoppedAction("git-worktree-removal-candidate-drift")).toBe(
      "worktree 상태가 승인 이후 변경되었습니다. 새 감사를 실행하고 다시 승인하세요.",
    );
    expect(removalStoppedAction("git-worktree-removal-command-failed")).toBe(
      "Git worktree 제거 명령이 실패했습니다. worktree가 잠겨 있거나 사용 중인지 확인한 뒤 새 감사부터 진행하세요.",
    );
    expect(removalStoppedAction("git-worktree-removal-post-verification-failed")).toBe(
      "제거 후 경로·등록·브랜치 보존을 모두 확인하지 못했습니다. Git worktree 목록과 브랜치를 확인하고 추가 제거를 중단하세요.",
    );

    const injected = "/Users/example/private-worktree: remove failed";
    const fallback =
      "제거 결과 검증이 불완전합니다. Git worktree 목록과 브랜치를 확인하고 추가 제거를 중단하세요.";
    expect(removalStoppedAction(injected)).toBe(fallback);
    expect(removalStoppedAction(null)).toBe(fallback);
    expect(removalStoppedAction("")).toBe(fallback);
    expect(removalStoppedAction("toString")).toBe(fallback);
    expect(removalStoppedAction("constructor")).toBe(fallback);
    expect(removalStoppedAction(injected)).not.toContain(injected);
  });

  it("keeps the existing planning and removal authority behind accessible feedback", () => {
    const source = readSource("src/lib/GitWorktreeCleanup.svelte");

    expect(source).toContain('role="alert"');
    expect(source).toContain("api.planStaleGitWorktrees(root, references)");
    expect(source).toContain("api.removeStaleGitWorktrees(");
    expect(source).toContain("evidenceGapActions(entry.blockers)");
    expect(source).toContain("removalStoppedAction(removal.result.stopped_reason)");
  });
});
