import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

const screens = [
  "CloudArchive.svelte",
  "IcloudLocalEviction.svelte",
  "OrphanCleanup.svelte",
  "BrewCleanup.svelte",
  "Cleanup.svelte",
  "GitWorktreeCleanup.svelte",
  "Organize.svelte",
  "Inventory.svelte",
  "Settings.svelte",
  "Duplicates.svelte",
  "TopFiles.svelte",
  "Treemap.svelte",
  "ContainerOrphanCleanup.svelte",
];

const forbidden = [
  /File Provider/i,
  /provider\s+attestation/i,
  /\battestation\b/i,
  /\badmission\b/i,
  /\bmetadata\b/i,
  /메타데이터/i,
  /Info\.plist/i,
  /Application Support/i,
  /Keychain/i,
  /OAuth/i,
  /quota API/i,
  /\bGoal\b/i,
  /\bADR\b/i,
  /\bLineage\b/i,
  /\bEntity\b/i,
  /fingerprint/i,
  /ubiquitous/i,
  /materialization/i,
  /staged item/i,
  /fetch\/create/i,
  /sync-up/i,
  /sync-down/i,
  /eviction/i,
  /축출/i,
  /영수증/i,
  /증거/i,
  /active-use/i,
  /Git 등록 해제/i,
  /분리된 HEAD/i,
  /VM 저장소/i,
  /tagged image/i,
  /dangling 이미지/i,
  /volume·/i,
  /지문을 검증합니다/i,
  /온톨로지/i,
  /아티팩트/i,
  /인벤토리/i,
  /모델/i,
  /APFS/i,
  /TOCTOU/i,
  /candidate_set_sha256/i,
  /\b런타임\b/i,
  /승인 문구\(지문\)/i,
];

function readScreen(name: string): string {
  return readFileSync(resolve(repositoryRoot, "src/lib", name), "utf8");
}

function quotedValues(value: string): string[] {
  return [...value.matchAll(/(?:"([^"\\]*(?:\\.[^"\\]*)*)"|'([^'\\]*(?:\\.[^'\\]*)*)')/g)].map(
    (match) => match[1] ?? match[2] ?? "",
  );
}

function customerAttributeValues(tag: string): string[] {
  return [...tag.matchAll(/(?:aria-label|title|placeholder|alt)\s*=\s*["']([^"']*)["']/gi)].map((match) => match[1] ?? "");
}

/** Extract customer-visible text while ignoring script implementation details. */
function visibleText(source: string): string {
  const markup = source.replace(/<script\b[\s\S]*?<\/script>/gi, "").replace(/<style\b[\s\S]*?<\/style>/gi, "");
  const text = markup
    .replace(/<[^>]+>/g, (tag) => customerAttributeValues(tag).join(" "))
    .replace(/\{([\s\S]*?)\}/g, (_, expression: string) =>
      quotedValues(expression).filter((value) => /\s|[가-힣]/.test(value)).join(" "),
    )
    .replace(/\s+/g, " ");
  return text;
}

describe("customer copy boundary", () => {
  it("does not expose implementation terms in visible screen copy", () => {
    for (const screen of screens) {
      const text = visibleText(readScreen(screen));
      for (const pattern of forbidden) {
        expect(text, `${screen} exposes ${pattern}`).not.toMatch(pattern);
      }
    }
  });

  it("makes every warning, error, and notice tell the customer what to do", () => {
    const action = /(확인|다시|선택|진행|기다|보관|복원|회수|승인|시도|취소|확보|허용|시작|멈추|새로)/;
    for (const screen of screens) {
      const source = readScreen(screen);
      for (const match of source.matchAll(/<p\b([^>]*)>([\s\S]*?)<\/p>/gi)) {
        const attributes = match[1] ?? "";
        if (!/(warning|error|notice|role=["'](?:alert|status)["'])/i.test(attributes)) continue;
        const text = visibleText(match[0]);
        if (!text.trim()) continue;
        expect(text, `${screen} has non-actionable guidance`).toMatch(action);
      }
    }
  });
});
