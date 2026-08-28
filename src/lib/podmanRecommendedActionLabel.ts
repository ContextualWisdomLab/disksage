export function podmanRecommendedActionLabel(kind: string): string {
  switch (kind) {
    case "review_unused_images":
      return "재생성 가능한 미사용 이미지 검토";
    case "review_unused_volumes":
      return "사용하지 않는 Podman 저장 공간 검토";
    case "restore_guest_headroom":
      return "Podman 환경의 여유 공간 확보";
    case "investigate_api":
      return "Podman 연결 상태 확인";
    case "review_guest_trim":
      return "Podman 환경의 저장 공간 정리 상태 확인";
    case "review_stopped_containers":
      return "중지된 Podman 작업 검토";
    default:
      return "Podman 저장 공간 확인";
  }
}
