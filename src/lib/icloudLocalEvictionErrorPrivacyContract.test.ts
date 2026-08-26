import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  ICLOUD_EVICTION_EXECUTION_FAILURE,
  ICLOUD_FILE_SELECTION_FAILURE,
  ICLOUD_RESULT_RECORD_FAILURE,
  ICLOUD_STATE_INSPECTION_FAILURE,
} from "./icloudLocalEvictionFeedback";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function readSource(path: string): string {
  return readFileSync(resolve(repositoryRoot, path), "utf8");
}

describe("iCloud local eviction privacy-safe failure feedback", () => {
  it("never renders arbitrary thrown or record-persistence error text", () => {
    const source = readSource("src/lib/IcloudLocalEviction.svelte");

    expect(source).not.toContain("String(e)");
    expect(source).not.toContain("{eviction.result_record_error}");
    expect(source).toContain("ICLOUD_FILE_SELECTION_FAILURE");
    expect(source).toContain("ICLOUD_STATE_INSPECTION_FAILURE");
    expect(source).toContain("ICLOUD_EVICTION_EXECUTION_FAILURE");
    expect(source).toContain("ICLOUD_RESULT_RECORD_FAILURE");
  });

  it("keeps every bounded failure path-free and action-oriented", () => {
    for (const message of [
      ICLOUD_FILE_SELECTION_FAILURE,
      ICLOUD_STATE_INSPECTION_FAILURE,
      ICLOUD_EVICTION_EXECUTION_FAILURE,
      ICLOUD_RESULT_RECORD_FAILURE,
    ]) {
      expect(message).not.toMatch(/(?:\/Users\/|[A-Za-z]:\\|file:\/\/)/);
      expect(message).toMatch(/(?:확인|보관|선택|판정)/);
    }
  });

  it("announces bounded failures without changing the operation authority", () => {
    const source = readSource("src/lib/IcloudLocalEviction.svelte");

    expect(source).toContain('role="alert"');
    expect(source).toContain("api.planIcloudLocalCopyEviction(cloudRoot, selectedPath)");
    expect(source).toContain("api.evictIcloudLocalCopy(");
  });
});
