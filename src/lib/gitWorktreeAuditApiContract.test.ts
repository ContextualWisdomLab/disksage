import { describe, expect, it } from "vitest";

import backendContract from "../../contracts/git-worktree-audit-v4.json";
import type { GitWorktreeAuditEntry, GitWorktreeAuditReport } from "./api";

const frontendSchemaKind: GitWorktreeAuditReport["schema_kind"] =
  "disksage.git-worktree-audit/v4";
const frontendMembershipFields: ReadonlyArray<keyof GitWorktreeAuditEntry> = [
  "completed_pull_request_commit",
  "open_pull_request_commit",
];

describe("Git worktree audit frontend contract", () => {
  it("matches the shared backend/runtime v4 contract", () => {
    expect(backendContract.schema_kind).toBe(frontendSchemaKind);
    expect(backendContract.version).toBe(4);
    expect(backendContract.entry_membership_fields).toEqual(frontendMembershipFields);
  });
});
