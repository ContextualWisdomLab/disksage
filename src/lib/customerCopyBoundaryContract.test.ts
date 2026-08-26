import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  boundedCloudArchiveErrorMessage,
  type CloudArchiveErrorOperation,
} from "./cloudArchiveErrorFeedback";
import {
  ICLOUD_EVICTION_EXECUTION_FAILURE,
  ICLOUD_FILE_SELECTION_FAILURE,
  ICLOUD_RESULT_RECORD_FAILURE,
  ICLOUD_STATE_INSPECTION_FAILURE,
  planBlockerActions,
  verificationBlockerActions,
} from "./icloudLocalEvictionFeedback";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const screens = [
  "src/lib/CloudArchive.svelte",
  "src/lib/IcloudLocalEviction.svelte",
  "src/lib/OrphanCleanup.svelte",
  "src/lib/BrewCleanup.svelte",
  "src/lib/Cleanup.svelte",
  "src/lib/GitWorktreeCleanup.svelte",
  "src/lib/Organize.svelte",
];
const cloudOperations = [
  "initialize", "preview", "review", "copy", "cancel", "provider-api-copy", "adopt", "attest",
  "reconcile", "icloud-health", "finder-copy-cancel", "provider-sync", "provider-recovery", "evict",
  "capacity", "connect", "disconnect",
] as const satisfies readonly CloudArchiveErrorOperation[];

function visibleText(path: string): string {
  let source = readFileSync(resolve(repositoryRoot, path), "utf8");
  source = source.replace(/<script[\s\S]*?<\/script>/gi, "");
  source = source.replace(/<style[\s\S]*?<\/style>/gi, "");
  source = source.replace(/<[^>]+>/g, " ");
  for (let pass = 0; pass < 3; pass += 1) source = source.replace(/\{[^{}]*\}/g, " ");
  return source.replace(/\s+/g, " ").trim();
}

describe("customer copy boundary", () => {
  it("hides implementation terms from the customer-visible cloud screens", () => {
    const forbidden = [
      "File Provider", "provider", "attestation", "admission", "metadata", "Metadata",
      "Info.plist", "Application Support", "Keychain", "OAuth", "quota API", "Goal", "ADR",
      "Lineage", "Entity", "fingerprint", "ubiquitous", "materialization", "staged item",
      "fetch/create", "sync-up", "sync-down", "eviction", "축출", "영수증", "증거",
      "객체 지문", "active-use", "Git 등록 해제", "분리된 HEAD", "VM 저장소", "tagged image",
      "dangling 이미지", "volume·", "지문을 검증합니다", "온톨로지", "lineage_fingerprint",
    ];
    for (const path of screens) {
      const text = visibleText(path);
      for (const term of forbidden) expect(text, `${path} exposes ${term}`).not.toContain(term);
    }
  });

  it("gives a next action for every warning or error paragraph", () => {
    const action = /(확인|다시|선택|진행|기다|보관|복원|회수|승인|시도|취소|확보|허용|시작|멈추|새로)/;
    for (const path of screens) {
      const source = readFileSync(resolve(repositoryRoot, path), "utf8");
      const paragraphs = [...source.matchAll(/<p[^>]*class=["'][^"']*(?:warning|error|notice)[^"']*["'][^>]*>([\s\S]*?)<\/p>/gi)];
      for (const paragraph of paragraphs) {
        const text = paragraph[1].replace(/<[^>]+>/g, " ").replace(/\{[^{}]*\}/g, " ");
        if (!text.trim()) continue;
        expect(text, `${path} warning lacks a next action`).toMatch(action);
      }
    }
  });

  it("keeps reusable customer feedback actionable and implementation-neutral", () => {
    const forbidden = /(File Provider|provider|attestation|admission|metadata|Info\.plist|Application Support|Keychain|OAuth|quota API|Goal|ADR|Lineage|Entity|fingerprint|ubiquitous|materialization|staged item|fetch\/create|sync-up|sync-down|eviction|축출|영수증|증거|active-use|온톨로지)/i;
    const action = /(확인|다시|선택|진행|기다|보관|복원|회수|승인|시도|취소|확보|허용|시작|멈추|새로)/;
    const messages = [
      ...cloudOperations.map((operation) => boundedCloudArchiveErrorMessage(operation, "backend detail")),
      ICLOUD_FILE_SELECTION_FAILURE,
      ICLOUD_STATE_INSPECTION_FAILURE,
      ICLOUD_EVICTION_EXECUTION_FAILURE,
      ICLOUD_RESULT_RECORD_FAILURE,
      ...planBlockerActions(["icloud-file-provider-native-status-unavailable", "unknown-blocker"]),
      ...verificationBlockerActions(["unknown-blocker"]),
    ];
    for (const message of messages) {
      expect(message).not.toMatch(forbidden);
      expect(message).toMatch(action);
    }
  });
});
