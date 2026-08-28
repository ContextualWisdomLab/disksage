import { describe, expect, it } from "vitest";
import { podmanRecommendedActionLabel } from "./podmanRecommendedActionLabel";

describe("podmanRecommendedActionLabel", () => {
  it.each([
    ["review_unused_images", "재생성 가능한 미사용 이미지 검토"],
    ["review_unused_volumes", "사용하지 않는 Podman 저장 공간 검토"],
    ["restore_guest_headroom", "Podman 환경의 여유 공간 확보"],
    ["investigate_api", "Podman 연결 상태 확인"],
    ["review_guest_trim", "Podman 환경의 저장 공간 정리 상태 확인"],
    ["review_stopped_containers", "중지된 Podman 작업 검토"],
  ])("maps %s to bounded customer guidance", (kind, expected) => {
    expect(podmanRecommendedActionLabel(kind)).toBe(expected);
  });

  it("fails closed to generic customer guidance for an unknown action kind", () => {
    expect(podmanRecommendedActionLabel("future_internal_action")).toBe("Podman 저장 공간 확인");
  });
});
