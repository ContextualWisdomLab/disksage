export type Verdict = "safe" | "caution" | "keep" | "unrated";

export interface VerdictBadge {
  label: string;
  cls: string;
  title: string;
}

/** 자동 위험도 자문 결과를 배지로 표시하며, 미판정에는 삭제 권한을 부여하지 않는다. */
export function verdictBadge(v: Verdict | string): VerdictBadge {
  switch (v) {
    case "safe":
      return { label: "낮은 위험", cls: "badge-safe", title: "자동 자문: 낮은 위험 — 작업 전 검증 필요" };
    case "caution":
      return { label: "주의", cls: "badge-caution", title: "삭제 주의 — 확인 권장 (자문)" };
    case "keep":
      return { label: "보관", cls: "badge-keep", title: "보관 권장 (자문)" };
    default:
      return { label: "미판정", cls: "badge-unrated", title: "판정 없음 — 작업 전 직접 확인 필요" };
  }
}
