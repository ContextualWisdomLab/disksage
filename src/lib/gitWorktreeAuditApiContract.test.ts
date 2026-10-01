import { describe, expect, it } from "vitest";

import backendContract from "../../contracts/git-worktree-audit-v5.json";
import type { GitWorktreeAuditEntry, GitWorktreeAuditReport } from "./api";

const frontendSchemaKind: GitWorktreeAuditReport["schema_kind"] =
  "disksage.git-worktree-audit/v5";
const frontendMembershipFields: ReadonlyArray<keyof GitWorktreeAuditEntry> = [
  "completed_pull_request_commit",
  "open_pull_request_commit",
];

describe("Git worktree audit frontend contract", () => {
  it("matches the shared backend/runtime v5 contract", () => {
    expect(backendContract.schema_kind).toBe(frontendSchemaKind);
    expect(backendContract.version).toBe(5);
    expect(backendContract.path_fingerprint_algorithm).toBe(
      "disksage.git-worktree-path/blake3-v2",
    );
    expect(backendContract.entry_fingerprint_algorithm).toBe(
      "disksage.git-worktree-entry/blake3-v3",
    );
    expect(backendContract.entry_membership_fields).toEqual(frontendMembershipFields);
  });
});
