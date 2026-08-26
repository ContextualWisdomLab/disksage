export type Verdict = "safe" | "caution" | "keep" | "unrated";

export interface VerdictBadge {
  label: string;
  cls: string;
  title: string;
}

/** Convert a bounded safety verdict into customer guidance without exposing the judging service. */
export function verdictBadge(v: Verdict | string): VerdictBadge {
  switch (v) {
    case "safe":
      return { label: "낮은 위험", cls: "badge-safe", title: "위험이 낮아 보입니다. 작업 전에 내용을 확인하세요." };
    case "caution":
      return { label: "주의", cls: "badge-caution", title: "주의가 필요합니다. 내용을 확인한 뒤 결정하세요." };
    case "keep":
      return { label: "보관", cls: "badge-keep", title: "보관을 권장합니다. 삭제하지 마세요." };
    default:
      return { label: "미판정", cls: "badge-unrated", title: "자동 판단을 사용할 수 없습니다. 내용을 직접 확인하세요." };
  }
}
