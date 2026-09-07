export type InventoryFailureKind =
  | "inventory-load"
  | "ontology-coherence"
  | "user-rules"
  | "model-status"
  | "model-download"
  | "unknown-extension-insight"
  | "unknown-summary";

const INVENTORY_FAILURE_MESSAGES: Record<InventoryFailureKind, string> = {
  "inventory-load":
    "인벤토리 집계에 실패했습니다. 스캔 대상 폴더의 접근 권한을 확인하고 스캔을 다시 실행한 뒤 집계하세요.",
  "ontology-coherence":
    "온톨로지 정합성 확인에 실패했습니다. DiskSage 리소스와 설정을 확인한 뒤 인벤토리를 다시 집계하세요.",
  "user-rules":
    "규칙 파일을 불러오지 못했습니다. DiskSage 데이터 폴더의 규칙 파일 권한과 형식을 확인한 뒤 인벤토리를 다시 집계하세요.",
  "model-status":
    "모델 상태를 확인하지 못했습니다. 모델 다운로드 여부를 다시 확인하거나 잠시 후 상태를 새로고침하세요.",
  "model-download":
    "모델 다운로드에 실패했습니다. 네트워크 연결과 DiskSage 데이터 폴더의 여유 공간을 확인한 뒤 다시 다운로드하세요.",
  "unknown-extension-insight":
    "미분류 확장자 자문에 실패했습니다. 인벤토리는 그대로 사용할 수 있으며 필요하면 다시 집계해 자문을 재시도하세요.",
  "unknown-summary":
    "미분류 요약에 실패했습니다. 모델 설치 상태를 확인한 뒤 요약을 다시 실행하세요.",
};

export function inventoryFailureMessage(kind: InventoryFailureKind, cause?: unknown): string {
  void cause;
  return INVENTORY_FAILURE_MESSAGES[kind];
}

export function isCurrentInventoryRequest(
  requestedRoot: string,
  requestedGeneration: number,
  currentRoot: string | null,
  currentGeneration: number,
): boolean {
  return requestedRoot === currentRoot && requestedGeneration === currentGeneration;
}

export async function requestUnknownExtensionInsights<T>(
  samples: readonly string[],
  reason: (samples: string[]) => Promise<T[]>,
): Promise<T[] | null> {
  if (samples.length === 0) return null;
  return reason([...samples]);
}
