import { describe, expect, it } from "vitest";

import { productionTimeConfidenceLabel } from "./productionTimeConfidenceLabel";

describe("productionTimeConfidenceLabel", () => {
  it("does not present estimated production dates as confirmed", () => {
    expect(productionTimeConfidenceLabel("high")).toBe("생산일 확인됨");
    expect(productionTimeConfidenceLabel("medium")).toBe("생산일 추정·중간 확신");
    expect(productionTimeConfidenceLabel("low")).toBe("생산일 추정·낮은 확신");
    expect(productionTimeConfidenceLabel("unknown")).toBe("생산일 미확인");
  });

  it("fails closed for an unrecognized backend value", () => {
    expect(productionTimeConfidenceLabel("filename:path-token")).toBe("생산일 미확인");
  });
});
