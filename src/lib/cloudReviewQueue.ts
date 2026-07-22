import type { CloudCandidate, CloudReviewDecision } from "./api";

export const CLOUD_REVIEW_PAGE_SIZE = 20;

export type CloudReviewQueueFilter =
  | "all"
  | "unreviewed"
  | "approved"
  | "held"
  | "blocked"
  | "ready";

export type CloudReviewQueueSort = "bytes-desc" | "production-asc" | "production-desc";

export type CloudReviewQueueState = Exclude<CloudReviewQueueFilter, "all">;

export interface CloudReviewQueueStats {
  total: number;
  totalBytes: number;
  reviewable: number;
  reviewableBytes: number;
  reviewed: number;
  reviewedBytes: number;
  unreviewed: number;
  unreviewedBytes: number;
  approved: number;
  held: number;
  blocked: number;
  blockedBytes: number;
  ready: number;
}

export interface CloudReviewQueuePage {
  items: CloudCandidate[];
  page: number;
  totalPages: number;
  startIndex: number;
  endIndex: number;
  totalItems: number;
}

const CLOUD_DECISION_REASON_LABELS: Readonly<Record<string, string>> = {
  "archive-contains-encrypted-entries": "압축 파일에 암호화된 항목이 있음",
  "archive-contains-recording-media": "압축 파일에 녹음·영상 파일이 있음",
  "archive-contains-secret-like-path": "압축 내부 경로에 비밀정보로 보이는 이름이 있음",
  "archive-contains-structured-data": "압축 파일에 구조화 데이터가 있음",
  "archive-contains-unsafe-entry-path": "압축 내부에 안전하지 않은 경로가 있음",
  "archive-index-unreadable": "압축 파일의 목록을 안전하게 읽을 수 없음",
  "dataset-quality-warning-present": "데이터셋 품질 경고가 있음",
  "dataset-schema-profile-incomplete": "데이터셋 스키마 표본 검사가 불완전함",
  "dataset-schema-profile-missing": "데이터셋 스키마 정보를 확인하지 못함",
  "dataset-sensitive-column-name-detected": "민감정보로 보이는 데이터셋 열 이름이 있음",
  "destination-account-scope-unknown": "목적지 클라우드 계정 범위를 확인해야 함",
  "destination-exists": "같은 목적지 파일이 이미 있음",
  "download-origin-needs-destination-review": "다운로드 출처와 보관 목적지의 적절성 확인이 필요함",
  "embedded-and-filename-date-conflict": "내장 생산일과 파일명 날짜가 서로 다름",
  "embedded-date-differs-from-filename-publication-month": "내장 생산일과 파일명 발행월이 다름",
  "embedded-metadata-contains-geolocation": "내장 메타데이터에 위치정보가 있음",
  "embedded-metadata-context-may-be-confidential": "내장 메타데이터 맥락에 기밀정보 가능성이 있음",
  "embedded-metadata-may-contain-personal-context": "내장 메타데이터에 개인정보 맥락 가능성이 있음",
  "embedded-metadata-probe-incomplete": "내장 메타데이터 검사가 완전하지 않음",
  "embedded-production-date-after-filesystem-modified": "내장 생산일이 파일 수정 시각보다 늦음",
  "embedded-production-date-confidence-not-high": "내장 생산일의 신뢰도가 높지 않음",
  "embedded-production-date-conflict": "여러 내장 생산일 증거가 서로 다름",
  "embedded-production-date-known-template-default": "내장 생산일이 알려진 템플릿 기본값과 같음",
  "exact-duplicate-content-needs-canonical-selection": "내용이 같은 파일 중 대표본을 선택해야 함",
  "exact-duplicate-content-probe-incomplete": "정확 중복 검사가 완전하지 않음",
  "filename-contains-geolocation": "파일명에 위치정보로 보이는 값이 있음",
  "filename-context-may-be-confidential": "파일명 맥락에 기밀정보 가능성이 있음",
  "incomplete-download": "다운로드가 완료되지 않은 파일임",
  "multipart-archive-atomic-copy-required": "분할 압축 전체를 함께 처리해야 함",
  "multipart-archive-member": "분할 압축의 일부 파일임",
  "opaque-container-content-uninspected": "컨테이너 내부 내용을 확인하지 못함",
  "organization-cloud-sensitive-context-needs-explicit-tenant-approval": "조직 클라우드에 민감 맥락을 보관할 명시적 확인이 필요함",
  "personal-cloud-sensitive-context-needs-explicit-approval": "개인 클라우드에 민감 맥락을 보관할 명시적 확인이 필요함",
  "production-date-not-from-embedded-metadata": "생산일을 내장 메타데이터에서 확인하지 못함",
  "recording-may-contain-sensitive-speech": "녹음·영상에 민감한 대화가 포함될 수 있음",
  "shared-destination-access-needs-review": "공유 목적지의 접근 범위를 확인해야 함",
  "spreadsheet-content-needs-review": "스프레드시트 내용의 민감성 확인이 필요함",
  "spreadsheet-quality-warning-present": "스프레드시트 품질 경고가 있음",
  "spreadsheet-schema-profile-incomplete": "스프레드시트 스키마 표본 검사가 불완전함",
  "spreadsheet-schema-profile-missing": "스프레드시트 스키마 정보를 확인하지 못함",
  "spreadsheet-sensitive-column-name-detected": "민감정보로 보이는 스프레드시트 열 이름이 있음",
  "structured-data-may-contain-personal-data": "구조화 데이터에 개인정보가 포함될 수 있음",
};

export function cloudDecisionReasonLabel(reason: string): string {
  return CLOUD_DECISION_REASON_LABELS[reason] ?? reason;
}

export function candidateReviewDecision(
  candidate: CloudCandidate,
  decisions: CloudReviewDecision[],
): CloudReviewDecision | null {
  return decisions.find((decision) =>
    decision.candidate_fingerprint === candidate.metadata_fingerprint
  ) ?? null;
}

export function matchingReviewDecision(
  candidate: CloudCandidate,
  decisions: CloudReviewDecision[],
): CloudReviewDecision | null {
  const decision = candidateReviewDecision(candidate, decisions);
  const reviewedBy = decision?.reviewed_by?.trim() ?? "";
  const rationale = decision?.rationale?.trim() ?? "";
  const attributedHumanDecision = decision?.version === 2
    && reviewedBy.startsWith("human:")
    && rationale.length > 0;
  return decision?.review_fingerprint === candidate.review_fingerprint && attributedHumanDecision
    ? decision
    : null;
}

export function cloudReviewQueueState(
  candidate: CloudCandidate,
  decisions: CloudReviewDecision[],
): CloudReviewQueueState {
  if (candidate.blocked_reason !== null) return "blocked";
  if (!candidate.requires_review) return "ready";
  return matchingReviewDecision(candidate, decisions)?.disposition ?? "unreviewed";
}

export function cloudReviewQueueStats(
  candidates: CloudCandidate[],
  decisions: CloudReviewDecision[],
): CloudReviewQueueStats {
  const stats: CloudReviewQueueStats = {
    total: candidates.length,
    totalBytes: 0,
    reviewable: 0,
    reviewableBytes: 0,
    reviewed: 0,
    reviewedBytes: 0,
    unreviewed: 0,
    unreviewedBytes: 0,
    approved: 0,
    held: 0,
    blocked: 0,
    blockedBytes: 0,
    ready: 0,
  };
  for (const candidate of candidates) {
    stats.totalBytes += candidate.bytes;
    const state = cloudReviewQueueState(candidate, decisions);
    if (state === "blocked") {
      stats.blocked += 1;
      stats.blockedBytes += candidate.bytes;
      continue;
    }
    if (state === "ready") {
      stats.ready += 1;
      continue;
    }
    stats.reviewable += 1;
    stats.reviewableBytes += candidate.bytes;
    if (state === "unreviewed") {
      stats.unreviewed += 1;
      stats.unreviewedBytes += candidate.bytes;
    } else {
      stats.reviewed += 1;
      stats.reviewedBytes += candidate.bytes;
      stats[state] += 1;
    }
  }
  return stats;
}

function lexicalCompare(left: string, right: string): number {
  if (left < right) return -1;
  if (left > right) return 1;
  return 0;
}

function stableCandidateTieBreak(left: CloudCandidate, right: CloudCandidate): number {
  return lexicalCompare(left.relative_path, right.relative_path)
    || lexicalCompare(left.metadata_fingerprint, right.metadata_fingerprint);
}

export function filterCloudReviewQueue(
  candidates: CloudCandidate[],
  decisions: CloudReviewDecision[],
  filter: CloudReviewQueueFilter,
  reason: string,
  sort: CloudReviewQueueSort,
): CloudCandidate[] {
  const filtered = candidates.filter((candidate) => {
    if (filter !== "all" && cloudReviewQueueState(candidate, decisions) !== filter) return false;
    return reason === "" || candidate.review_reasons.includes(reason);
  });
  return [...filtered].sort((left, right) => {
    if (sort === "bytes-desc") {
      if (left.bytes !== right.bytes) return left.bytes < right.bytes ? 1 : -1;
      return stableCandidateTieBreak(left, right);
    }
    if (sort === "production-asc") {
      if (left.production_time_ms !== right.production_time_ms) {
        return left.production_time_ms < right.production_time_ms ? -1 : 1;
      }
      return stableCandidateTieBreak(left, right);
    }
    if (left.production_time_ms !== right.production_time_ms) {
      return left.production_time_ms < right.production_time_ms ? 1 : -1;
    }
    return stableCandidateTieBreak(left, right);
  });
}

export function cloudReviewReasons(candidates: CloudCandidate[]): string[] {
  return [...new Set(candidates.flatMap((candidate) => candidate.review_reasons))].sort(lexicalCompare);
}

export function cloudReviewQueuePage(
  candidates: CloudCandidate[],
  requestedPage: number,
  pageSize = CLOUD_REVIEW_PAGE_SIZE,
): CloudReviewQueuePage {
  const safePageSize = Math.max(1, Math.floor(pageSize));
  const totalPages = Math.max(1, Math.ceil(candidates.length / safePageSize));
  const page = Math.min(totalPages, Math.max(1, Math.floor(requestedPage)));
  const start = (page - 1) * safePageSize;
  const items = candidates.slice(start, start + safePageSize);
  return {
    items,
    page,
    totalPages,
    startIndex: items.length === 0 ? 0 : start + 1,
    endIndex: start + items.length,
    totalItems: candidates.length,
  };
}
