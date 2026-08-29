import { describe, expect, it } from "vitest";

import type { GitWorktreeAuditEntry, GitWorktreeAuditReport } from "./api";

const backendSchemaKind: GitWorktreeAuditReport["schema_kind"] =
  "disksage.git-worktree-audit/v4";

const backendMembershipFields: Pick<
  GitWorktreeAuditEntry,
  "completed_pull_request_commit" | "open_pull_request_commit"
> = {
  completed_pull_request_commit: true,
  open_pull_request_commit: false,
};

describe("Git worktree audit frontend contract", () => {
  it("matches the backend v4 schema and PR membership fields", () => {
    expect(backendSchemaKind).toBe("disksage.git-worktree-audit/v4");
    expect(backendMembershipFields).toEqual({
      completed_pull_request_commit: true,
      open_pull_request_commit: false,
    });
  });
});
