import type { DupeGroup } from "./api";

/** 각 중복 그룹에서 최소 1개 사본이 보존되는지 검사.
 *  삭제 선택(toDelete)이 어떤 그룹의 모든 사본을 포함하면 true(=삭제 차단). */
export function blocksDeletion(groups: DupeGroup[], toDelete: Set<string>): boolean {
  return groups.some((g) => g.paths.every((p) => toDelete.has(p)));
}
