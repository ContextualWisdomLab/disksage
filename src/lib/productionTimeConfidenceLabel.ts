/** Render production-date confidence without overstating estimated metadata as confirmed. */
export function productionTimeConfidenceLabel(confidence: string | null | undefined): string {
  switch (confidence) {
    case "high":
      return "생산일 확인됨";
    case "medium":
      return "생산일 추정·중간 확신";
    case "low":
      return "생산일 추정·낮은 확신";
    default:
      return "생산일 미확인";
  }
}
