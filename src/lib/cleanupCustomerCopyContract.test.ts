import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

// Keep this contract scoped to customer surfaces this Storybook/UX owner actually changes.
// Other product surfaces have separate canonical PR owners; asserting their current-main copy here
// would turn unrelated dependency movement into a false-negative gate.
const screenFiles = [
  "src/lib/Cleanup.svelte",
  "src/lib/IcloudLocalEviction.svelte",
  "src/lib/OrphanCleanup.svelte",
].map((path) => resolve(repositoryRoot, path));

function visibleText(filePath: string): string {
  const source = readFileSync(filePath, "utf8");
  const scriptEnd = source.indexOf("</script>");
  const styleStart = source.indexOf("<style>");
  if (scriptEnd < 0) throw new Error(`${filePath}: customer script marker is missing`);
  const markupEnd = styleStart > scriptEnd ? styleStart : source.length;
  return source
    .slice(scriptEnd + "</script>".length, markupEnd)
    .replace(/<[^>]*>/g, " ")
    .replace(/\{[\s\S]*?\}/g, " ");
}

function customerMarkup(filePath: string): string {
  const source = readFileSync(filePath, "utf8");
  const scriptEnd = source.indexOf("</script>");
  const styleStart = source.indexOf("<style>");
  if (scriptEnd < 0) throw new Error(`${filePath}: customer script marker is missing`);
  const markupEnd = styleStart > scriptEnd ? styleStart : source.length;
  return source
    .slice(scriptEnd + "</script>".length, markupEnd)
    .replace(/<!--[\s\S]*?-->/g, "");
}

function staticActionParagraphs(filePath: string): string[] {
  const source = readFileSync(filePath, "utf8");
  const paragraphs: string[] = [];
  const pattern = /<p[^>]*class=(?:"|')([^"']*)(?:"|')[^>]*>([\s\S]*?)<\/p>/g;
  for (const match of source.matchAll(pattern)) {
    if (!/(warning|error|notice)/.test(match[1]) || /[{@]/.test(match[2])) continue;
    const text = match[2].replace(/<[^>]*>/g, " ").replace(/\s+/g, " ").trim();
    if (text) paragraphs.push(text);
  }
  return paragraphs;
}

describe("customer copy contract", () => {
  it("does not expose implementation boundaries in Storybook UX-owned customer screens", () => {
    const forbidden = [
      "온톨로지", "계보", "attestation", "File Provider", "OAuth", "active-use",
      "APFS 공유 블록", "메타데이터 스캔", "dangling 이미지", "VM 저장소", "증거 공백",
      "분리된 HEAD", "Git 등록 해제", "사전 할당량 기준", "tag가 없는", "tagged image",
      "지문을 검증합니다", "승인 기록", "결과 기록", "계획 지문", "감사 기록", "dry-run", "LLM",
      "Info.plist", "Library 후보", "파일시스템 메타데이터", "Application Support",
      "Keychain", "원격 API", "quota API", "Apple 네이티브", "원격 quota", "iCloud 네이티브",
    ];
    for (const filePath of screenFiles) {
      const text = visibleText(filePath);
      for (const term of forbidden) expect(text, `${filePath}: ${term}`).not.toContain(term);
    }
  });

  it("keeps dynamic customer messages free of implementation vocabulary", () => {
    const forbiddenLiterals = [
      "File Provider", "provider item", "eviction permit",
      "대표 lineage", "lineage 연결", "access token", "refresh token", "Rust 내부", "exact record",
      "dangling 이미지", "ubiquitous identity", "Goal/ADR", "증거가 부족해",
    ];
    for (const filePath of screenFiles) {
      const markup = customerMarkup(filePath);
      for (const term of forbiddenLiterals) {
        expect(markup, `${filePath}: ${term}`).not.toContain(term);
      }
    }
  });

  it("uses bounded action guidance for static warnings and errors", () => {
    const nextAction = /(확인|다시|입력|선택|누르|비우|연결|진행|검토|승인|복원|이동|재시작|기다|조정|바꾸|재개|실행|확보|취소|보존|회수|미리보기|휴지통|스캔|시도|조건)/;
    for (const filePath of screenFiles) {
      for (const paragraph of staticActionParagraphs(filePath)) {
        expect(paragraph, `${filePath}: ${paragraph}`).toMatch(nextAction);
      }
    }
  });
});
