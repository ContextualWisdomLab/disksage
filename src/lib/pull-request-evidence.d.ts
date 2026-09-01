import "./api";

declare module "./api" {
  interface PullRequestEvidence {
    association_method: "exact-head" | "commit-associated";
  }
}
