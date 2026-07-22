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
  return decision?.review_fingerprint === candidate.review_fingerprint ? decision : null;
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
