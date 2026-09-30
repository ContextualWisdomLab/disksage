import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  ICLOUD_EVICTION_EXECUTION_FAILURE,
  ICLOUD_FILE_SELECTION_FAILURE,
  ICLOUD_STATE_INSPECTION_FAILURE,
  planBlockerActions,
} from "./icloudLocalEvictionFeedback";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(): string {
  return readFileSync(resolve(repositoryRoot, "src/lib/IcloudLocalEviction.svelte"), "utf8");
}

describe("iCloud local eviction safety UI", () => {
  it("explains local-current plus unconfirmed upload without exposing backend details", () => {
    const source = readSource();

    expect(source).toContain("로컬 최신본·업로드 미확인");
    expect(planBlockerActions(["icloud-upload-not-confirmed"])).toEqual([
      "iCloud 업로드가 완료될 때까지 기다린 뒤 다시 판정하세요.",
    ]);
    expect(source).toContain('role="status"');
    expect(source).not.toContain("String(e)");
    expect(source).not.toContain("result_record_error");
  });

  it("keeps customer next actions bounded for each native-state failure path", () => {
    const source = readSource();

    expect(source).toContain("ICLOUD_FILE_SELECTION_FAILURE");
    expect(source).toContain("ICLOUD_STATE_INSPECTION_FAILURE");
    expect(source).toContain("ICLOUD_EVICTION_EXECUTION_FAILURE");
    expect(ICLOUD_FILE_SELECTION_FAILURE).toContain("iCloud 파일");
    expect(ICLOUD_STATE_INSPECTION_FAILURE).toContain("iCloud 상태");
    expect(ICLOUD_EVICTION_EXECUTION_FAILURE).toContain("로컬 사본");
    expect(source).toContain("planBlockerActions(plan.blockers.filter");
    expect(source).toContain(
      'blocker !== "human-local-eviction-approval-required"',
    );
    expect(planBlockerActions(["icloud-file-provider-native-status-unavailable"])).toEqual([
      "iCloud 상태 확인이 끝나지 않았습니다. 잠시 후 다시 확인하세요.",
    ]);
  });
});
