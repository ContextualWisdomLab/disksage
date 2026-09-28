import type {
  CloudAttestationOutput,
  CloudCopyOutput,
  CloudSourceEvictionOutput,
} from "./api";

export interface CloudLineageExportNode {
  id: string;
  kind: "source" | "metadata" | "archive" | "provider" | "provider-item" | "receipt" | "goal" | "eviction";
  status: string;
}

export interface CloudLineageExportEdge {
  subject: string;
  predicate: string;
  object: string;
}

export interface CloudLineageExport {
  schema: "disksage.cloud-lineage";
  version: 1;
  generated_at_ms: number;
  content_id: string;
  production_time_source: string;
  production_time_confidence: string;
  metadata_precedence: readonly [
    "embedded-metadata",
    "explicit-filename-date",
    "filesystem-created",
    "filesystem-modified",
  ];
  provider: CloudCopyOutput["receipt"]["provider"];
  provider_sync_state: CloudAttestationOutput["evidence"]["sync_state"];
  remote_object_id: string | null;
  remote_revision: string | null;
  remote_location_bound: boolean | null;
  blockers: string[];
  local_paths_included: false;
  nodes: CloudLineageExportNode[];
  edges: CloudLineageExportEdge[];
}

const metadataPrecedence = [
  "embedded-metadata",
  "explicit-filename-date",
  "filesystem-created",
  "filesystem-modified",
] as const;

const nodeId = (kind: CloudLineageExportNode["kind"], value: string): string =>
  `${kind}:${value}`;

/** Build a stable, path-free lineage graph from the current receipt and evidence. */
export function buildCloudLineageExport(
  copied: CloudCopyOutput,
  attestation: CloudAttestationOutput | null,
  eviction: CloudSourceEvictionOutput | null,
  generatedAtMs = Date.now(),
): CloudLineageExport | null {
  const lineage = copied.receipt.lineage;
  if (!lineage || !copied.receipt.lineage_fingerprint) return null;

  const source = nodeId("source", copied.receipt.candidate_fingerprint);
  const metadata = nodeId("metadata", copied.receipt.candidate_fingerprint);
  const archive = nodeId("archive", copied.receipt.lineage_fingerprint);
  const provider = nodeId("provider", copied.receipt.provider);
  const receipt = nodeId("receipt", copied.receipt.receipt_id);
  const goal = nodeId("goal", copied.receipt.receipt_id);
  const remoteObject = attestation?.evidence.remote_content?.object_id
    ? nodeId("provider-item", attestation.evidence.remote_content.object_id)
    : null;
  const evictionNode = eviction ? nodeId("eviction", eviction.approval.approval_id) : null;
  const syncState = attestation?.evidence.sync_state ?? "unknown";
  const blockers = [...(attestation?.blockers ?? [])].sort();

  const nodes: CloudLineageExportNode[] = [
    { id: source, kind: "source", status: copied.receipt.copy_verified ? "verified" : "blocked" },
    { id: metadata, kind: "metadata", status: lineage.production_time_confidence },
    { id: archive, kind: "archive", status: copied.goal_state },
    { id: provider, kind: "provider", status: syncState },
    { id: receipt, kind: "receipt", status: copied.receipt.copy_verified ? "verified" : "blocked" },
    { id: goal, kind: "goal", status: copied.goal_status ?? "unknown" },
  ];
  if (remoteObject) {
    nodes.splice(4, 0, { id: remoteObject, kind: "provider-item", status: "identified" });
  }
  if (eviction) {
    nodes.push({
      id: nodeId("eviction", eviction.approval.approval_id),
      kind: "eviction",
      status: eviction.goal_state,
    });
  }

  const edges: CloudLineageExportEdge[] = [
    { subject: source, predicate: "has-metadata-evidence", object: metadata },
    { subject: source, predicate: "archived-to", object: archive },
    { subject: archive, predicate: "managed-by", object: provider },
    { subject: archive, predicate: "has-copy-receipt", object: receipt },
    { subject: receipt, predicate: "projects-goal", object: goal },
  ];
  if (attestation) {
    edges.push({ subject: receipt, predicate: "attested-by", object: provider });
  }
  if (remoteObject) {
    edges.push({ subject: provider, predicate: "has-provider-item", object: remoteObject });
    edges.push({ subject: receipt, predicate: "matches-provider-item", object: remoteObject });
  }
  if (evictionNode) {
    edges.push({ subject: goal, predicate: "authorizes", object: evictionNode });
  }

  return {
    schema: "disksage.cloud-lineage",
    version: 1,
    generated_at_ms: generatedAtMs,
    content_id: copied.receipt.candidate_fingerprint,
    production_time_source: lineage.production_time_source,
    production_time_confidence: lineage.production_time_confidence,
    metadata_precedence: metadataPrecedence,
    provider: copied.receipt.provider,
    provider_sync_state: syncState,
    remote_object_id: attestation?.evidence.remote_content?.object_id ?? null,
    remote_revision: attestation?.evidence.remote_content?.revision ?? null,
    remote_location_bound: attestation?.evidence.remote_content?.location_bound ?? null,
    blockers,
    local_paths_included: false,
    nodes,
    edges,
  };
}
