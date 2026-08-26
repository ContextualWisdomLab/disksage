type ProductionTimeConfidence = "high" | "medium" | "low" | "unknown";

/** Render production-date confidence without overstating estimated metadata as confirmed. */
export function productionTimeConfidenceLabel(confidence: ProductionTimeConfidence): string {
  return {
    high: "생산일 확인됨",
    medium: "생산일 추정·중간 확신",
    low: "생산일 추정·낮은 확신",
    unknown: "생산일 미확인",
  }[confidence];
}
