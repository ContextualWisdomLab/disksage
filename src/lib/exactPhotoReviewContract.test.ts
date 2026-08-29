import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("./ExactPhotoReview.svelte", import.meta.url), "utf8");

describe("exact photo review safety and accessibility contract", () => {
  it("requires direct typed approval and an explicit reason", () => {
    expect(source).toContain("quarantineApprovalReady(plan, approval, rationale)");
    expect(source).not.toContain("approval = plan.exact_approval_phrase");
  });

  it("requires an explicit keeper for every tie and does not admit near duplicates", () => {
    expect(source).toContain("!allSelected()");
    expect(source).toContain("남길 사진을 직접 선택하세요");
    expect(source).toContain("비슷해 보이는 사진은 자동 처리하지 않습니다");
  });

  it("lets customers compare different encodings without byte-duplicate prefiltering", () => {
    expect(source).toContain("사진 직접 선택");
    expect(source).toContain("selectedPaths = Array.isArray(chosen)");
    expect(source).toContain("auditExactPhotoDuplicates(selectedPaths)");
  });

  it("resets stale review evidence after a new scan and does not bind direct picks to the scan root", () => {
    expect(source).toContain("fingerprint !== reviewedCandidateFingerprint");
    expect(source).toContain("selectedPaths = duplicateCandidatePaths(duplicateGroups)");
    expect(source).toContain("audit = null; plan = null; receipt = null");
    expect(source).toContain("planExactPhotoDuplicateQuarantine(audit, selections)");
    expect(source).not.toContain("planExactPhotoDuplicateQuarantine(scannedRoot");
  });

  it("announces results and preserves keyboard-visible 44px controls", () => {
    expect(source).toContain('role="status" aria-live="polite"');
    expect(source).toContain('role="alert"');
    expect(source).toContain("min-height:44px");
    expect(source).toContain(":focus-visible");
  });
});
