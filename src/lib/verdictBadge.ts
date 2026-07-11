export type Verdict = "safe" | "caution" | "keep" | "unrated";

export interface VerdictBadge {
  label: string;
  cls: string;
  title: string;
}

/** LLM 삭제-안전 판정 → 배지 표시. 미상/미판정은 unrated로 폴백. 자문(advisory)일 뿐. */
export function verdictBadge(v: Verdict | string): VerdictBadge {
  switch (v) {
    case "safe":
      return { label: "안전", cls: "badge-safe", title: "삭제해도 안전 (자문)" };
    case "caution":
      return { label: "주의", cls: "badge-caution", title: "삭제 주의 — 확인 권장 (자문)" };
    case "keep":
      return { label: "보관", cls: "badge-keep", title: "보관 권장 (자문)" };
    default:
      return { label: "미판정", cls: "badge-unrated", title: "판정 없음 (모델 미설치 또는 추론 실패)" };
  }
}
