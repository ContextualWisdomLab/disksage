import { describe, expect, it } from "vitest";
import { buildCloudLineageExport } from "./cloudLineageExport";
import type { CloudAttestationOutput, CloudCopyOutput } from "./api";

const copied = {
  action: "copy-only",
  goal_state: "pending-provider-sync",
  goal_status: "blocked",
  receipt: {
    candidate_fingerprint: "a".repeat(64),
    receipt_id: "b".repeat(64),
    provider: "icloud",
    copy_verified: true,
    lineage_fingerprint: "c".repeat(64),
    lineage: {
      production_time_source: "embedded:com.apple.metadata:kMDItemFSCreationDate",
      production_time_confidence: "high",
    },
  },
} as unknown as CloudCopyOutput;

const attestation = {
  evidence: { sync_state: "pending-upload" },
  blockers: ["provider-sync-incomplete", "icloud-indexing-pending"],
} as unknown as CloudAttestationOutput;

describe("cloud lineage export", () => {
  it("exports path-free stable graph edges and blockers", () => {
    const exported = buildCloudLineageExport(copied, attestation, null, 123);

    expect(exported).not.toBeNull();
    expect(exported).toMatchObject({
      schema: "disksage.cloud-lineage",
      version: 1,
      generated_at_ms: 123,
      provider: "icloud",
      provider_sync_state: "pending-upload",
      remote_object_id: null,
      remote_revision: null,
      remote_location_bound: null,
      blockers: ["icloud-indexing-pending", "provider-sync-incomplete"],
      local_paths_included: false,
    });
    expect(exported?.edges.map((edge) => edge.predicate)).toEqual([
      "has-metadata-evidence",
      "archived-to",
      "managed-by",
      "has-copy-receipt",
      "projects-goal",
      "attested-by",
    ]);
  });

  it("adds the eviction relation only after a real eviction output exists", () => {
    const eviction = {
      goal_state: "source-evicted",
      approval: { approval_id: "d".repeat(64) },
    } as never;

    const exported = buildCloudLineageExport(copied, null, eviction, 456);
    expect(exported?.nodes.at(-1)).toEqual({
      id: `eviction:${"d".repeat(64)}`,
      kind: "eviction",
      status: "source-evicted",
    });
    expect(exported?.edges.at(-1)).toEqual({
      subject: `goal:${"b".repeat(64)}`,
      predicate: "authorizes",
      object: `eviction:${"d".repeat(64)}`,
    });
  });

  it("links an identified remote provider item without exposing a path", () => {
    const remote = {
      ...attestation,
      evidence: {
        sync_state: "complete",
        remote_content: { object_id: "provider-item-1", revision: "rev-2", location_bound: true },
      },
      blockers: [],
    } as unknown as CloudAttestationOutput;

    const exported = buildCloudLineageExport(copied, remote, null, 789);
    expect(exported?.remote_object_id).toBe("provider-item-1");
    expect(exported?.remote_revision).toBe("rev-2");
    expect(exported?.remote_location_bound).toBe(true);
    expect(exported?.nodes).toContainEqual({
      id: "provider-item:provider-item-1",
      kind: "provider-item",
      status: "identified",
    });
    expect(exported?.edges.map((edge) => edge.predicate)).toContain("matches-provider-item");
  });

  it("fails closed when a legacy receipt has no lineage fingerprint", () => {
    expect(
      buildCloudLineageExport(
        { ...copied, receipt: { ...copied.receipt, lineage_fingerprint: undefined } } as CloudCopyOutput,
        null,
        null,
      ),
    ).toBeNull();
  });
});
