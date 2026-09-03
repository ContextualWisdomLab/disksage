import type { PullRequestEvidence } from "./api";

// Compile-time wire-contract probe: Rust serializes the association proof that
// distinguishes an exact PR head from a commit-associated fallback.
const exactHeadEvidence: PullRequestEvidence = {
  number: 7,
  state: "OPEN",
  headRefName: "feature",
  headRefOid: "a".repeat(40),
  createdAtMs: 1,
  url: "https://github.com/example/repo/pull/7",
  association_method: "exact-head",
};

// Rust intentionally keeps the upstream PR state as an opaque String. The
// TypeScript boundary must therefore remain forward-compatible with a future
// GitHub state instead of narrowing the wire contract to today's literals.
const forwardCompatibleEvidence: PullRequestEvidence = {
  ...exactHeadEvidence,
  state: "FUTURE_STATE",
  association_method: "commit-associated",
};

void exactHeadEvidence;
void forwardCompatibleEvidence;
