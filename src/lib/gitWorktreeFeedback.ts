/** Customer guidance shown when the native folder picker cannot return a Git repository. */
export const GIT_WORKTREE_REPOSITORY_SELECTION_FAILURE =
  "Git 저장소를 선택하지 못했습니다. 폴더 접근 권한과 저장소 위치를 확인한 뒤 다시 선택하세요.";

/** Customer guidance shown when the native removal confirmation cannot be opened. */
export const GIT_WORKTREE_CONFIRMATION_FAILURE =
  "제거 확인 창을 열지 못했습니다. 다른 확인 창을 닫은 뒤 새 감사부터 다시 진행하세요.";

/** Customer guidance shown when DiskSage cannot produce a fresh read-only worktree audit. */
export const GIT_WORKTREE_AUDIT_FAILURE =
  "Git worktree 감사를 완료하지 못했습니다. 저장소 경로와 보존할 ref가 현재 로컬에서 해석되는지 확인한 뒤 다시 감사하세요.";

/** Customer guidance shown when an approved worktree removal cannot complete. */
export const GIT_WORKTREE_REMOVAL_FAILURE =
  "Git worktree 제거에 실패했습니다. 저장소 상태와 계획 지문을 다시 확인한 뒤 새 감사부터 진행하세요.";

/**
 * Customer guidance shown when the removal result exists but its immutable record did not persist.
 * The native persistence error is deliberately not reflected because it can contain local paths.
 */
export const GIT_WORKTREE_RESULT_RECORD_FAILURE =
  "제거 결과는 위와 같지만 기록을 저장하지 못했습니다. DiskSage 데이터 폴더의 권한과 여유 공간을 확인하고 이 화면의 결과를 별도로 보관하세요.";

const UNKNOWN_EVIDENCE_GAP_ACTION =
  "증거가 불완전합니다. 해당 worktree의 Git 상태와 활성 사용을 직접 확인한 뒤 다시 감사하세요.";

const UNKNOWN_REMOVAL_STOP_ACTION =
  "제거 결과 검증이 불완전합니다. Git worktree 목록과 브랜치를 확인하고 추가 제거를 중단하세요.";

const EVIDENCE_GAP_ACTIONS: Readonly<Record<string, string>> = {
  "worktree-path-evidence-incomplete":
    "worktree 경로를 안전하게 확인하지 못했습니다. Git 등록 상태와 디렉터리 접근 권한을 확인한 뒤 다시 감사하세요.",
  "git-status-evidence-incomplete":
    "변경 사항 여부를 확인하지 못했습니다. 해당 worktree에서 Git 상태를 확인한 뒤 다시 감사하세요.",
  "reference-containment-evidence-incomplete":
    "HEAD가 보존 ref에 포함되는지 확인하지 못했습니다. ref를 fetch하고 정확한 ref 이름을 입력한 뒤 다시 감사하세요.",
  "actor-cwd-evidence-incomplete":
    "현재 디렉터리 사용 여부를 확인하지 못했습니다. DiskSage와 터미널의 현재 디렉터리를 worktree 밖으로 옮긴 뒤 다시 감사하세요.",
  "size-evidence-incomplete":
    "worktree 크기를 끝까지 측정하지 못했습니다. 접근 권한과 디스크 상태를 확인한 뒤 다시 감사하세요.",
  "active-use-evidence-incomplete":
    "열린 파일과 프로세스 사용 여부를 확인하지 못했습니다. 관련 앱과 터미널을 닫은 뒤 다시 감사하세요.",
};

const REMOVAL_STOP_ACTIONS: Readonly<Record<string, string>> = {
  "git-worktree-removal-live-reaudit-failed":
    "실행 직전 재감사에 실패했습니다. 저장소 상태를 확인한 뒤 새 감사부터 진행하세요.",
  "git-worktree-removal-reference-drift":
    "보존 ref가 승인 이후 변경되었습니다. 최신 ref로 새 감사를 실행하고 다시 승인하세요.",
  "git-worktree-removal-candidate-drift":
    "worktree 상태가 승인 이후 변경되었습니다. 새 감사를 실행하고 다시 승인하세요.",
  "git-worktree-removal-command-failed":
    "Git worktree 제거 명령이 실패했습니다. worktree가 잠겨 있거나 사용 중인지 확인한 뒤 새 감사부터 진행하세요.",
  "git-worktree-removal-post-verification-failed":
    "제거 후 경로·등록·브랜치 보존을 모두 확인하지 못했습니다. Git worktree 목록과 브랜치를 확인하고 추가 제거를 중단하세요.",
};

function ownAction(
  actions: Readonly<Record<string, string>>,
  code: string,
  fallback: string,
): string {
  return Object.prototype.hasOwnProperty.call(actions, code) ? actions[code] : fallback;
}

/**
 * Convert native evidence-gap codes into deduplicated, bounded customer actions.
 * Unknown, inherited, or missing values never cross the UI boundary and still produce safe guidance.
 */
export function evidenceGapActions(codes: readonly string[]): string[] {
  if (codes.length === 0) return [UNKNOWN_EVIDENCE_GAP_ACTION];

  const actions: string[] = [];
  const seen = new Set<string>();
  for (const code of codes) {
    const action = ownAction(EVIDENCE_GAP_ACTIONS, code, UNKNOWN_EVIDENCE_GAP_ACTION);
    if (!seen.has(action)) {
      seen.add(action);
      actions.push(action);
    }
  }
  return actions;
}

/**
 * Convert a native removal stop code into a bounded stop-and-recheck action.
 * Unknown, inherited, or missing values never cross the UI boundary.
 */
export function removalStoppedAction(reason: string | null): string {
  if (!reason) return UNKNOWN_REMOVAL_STOP_ACTION;
  return ownAction(REMOVAL_STOP_ACTIONS, reason, UNKNOWN_REMOVAL_STOP_ACTION);
}
