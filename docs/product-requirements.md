# DiskSage Product Requirements Document

**Status:** Baseline for implementation and release review
**Snapshot:** 2026-08-25 (Asia/Seoul)
**Product:** DiskSage by ContextualWisdomLab

## Product outcome

DiskSage helps a single desktop user reclaim local disk space without making an
unverifiable deletion or cloud-eviction decision. It scans locally, explains
which evidence is missing, copies only through a bounded provider boundary, and
evicts a source only after an independently verifiable provider receipt.

The standalone desktop product must work without OAuth, an external LLM, or a
network service. Cloud desktop clients (iCloud, OneDrive, and Google Drive) are
optional destinations; their capacity, sync state, and receipts are distinct
from one another.

## Users and primary stories

1. **Relieve disk pressure safely.** As a desktop user, I can scan a volume and
   see which files, caches, and provider states consume space before any action.
2. **Understand a blocked transfer.** As a user, I can see the exact provider
   blocker, evidence timestamp, required bytes, and next bounded retry.
3. **Offload a verified candidate.** As a user, I can copy a candidate to a
   chosen cloud desktop client, verify identity and content, and receive a
   durable receipt before eviction becomes possible.
4. **Clean reversible local state.** As a user, I can reclaim an identity-bound,
   regenerable cache through a dry run, Trash move, journal, and rollback path.
   The narrow ADR-0002 incident policy may execute its reviewed, identity-bound
   cache roots without a second prompt; it is not a general path-based delete.
5. **Audit a decision.** As a user or operator, I can export a path-free
   lineage projection from source metadata through provider evidence, receipt,
   Goal projection, and eviction decision. The local compatibility envelope
   `NaruonFileLineageEnvelope` may retain source and destination paths for
   recovery; it is not a shareable export and callers must use the redacted
   projection before sending data outside the local trust boundary.

## Functional requirements

### FR-1: Metadata-first inventory

- Read-only scans identify stable content and filesystem metadata.
- Metadata precedence is fixed and testable: embedded production metadata first,
  then an unambiguous filename token, then filesystem creation time, with
  filesystem modification time only as the final fallback. A filename token
  such as `2026-04-28` or `251210` is secondary evidence only.
- Missing, malformed, or conflicting metadata is visible and never silently
  upgraded to ownership or eviction authority.
- Scans exclude provider-managed trees or use a provider-native metadata
  capability that proves a placeholder will not be materialized; unsupported or
  ambiguous provider state is surfaced as a blocker.

### FR-2: Provider state machine

After a copy has been verified, the successful post-copy Goal projection follows
this monotonic state machine:

`copy-verified → pending-provider-sync → provider-sync-confirmed → eviction-ready → source-evicted`

`local-current` with `is_uploaded=false` is `pending-upload`; it cannot issue an
eviction permit. Provider timeout, quota/auth uncertainty, stale evidence,
insufficient headroom, or an incomplete receipt fail closed.

The Goal state machine is intentionally success-only and begins at
`copy-verified`. Pre-copy, failed, and cancelled observations are separate
candidate/provider evidence states. Provider-sync evidence drives the listed
Goal transitions, but it does not grant eviction authority until the current
receipt, identity, provider attestation, and every eviction gate are complete.

The provider-state projection is fixed as follows. Every state except `complete`
remains `pending-provider-sync` and has no eviction permit; in particular,
`content-mismatch`, `unknown`, and `excluded-from-sync` can never project to
`provider-sync-confirmed`.

| Provider state | Goal projection | Eviction permit | Required next action |
| --- | --- | --- | --- |
| `complete` | `provider-sync-confirmed`, or `eviction-ready` when every independent gate is valid | Only when the current receipt, identity, attestation, and source gates pass | Reconcile the receipt and review the source action |
| `pending-upload` | `pending-provider-sync` | No | Wait for provider upload, then collect fresh evidence |
| `not-ubiquitous` | `pending-provider-sync` | No | Make the destination available and collect fresh evidence |
| `not-local-current` | `pending-provider-sync` | No | Restore a local current copy before verification |
| `uploading` | `pending-provider-sync` | No | Wait for upload completion and refresh status |
| `excluded-from-sync` | `pending-provider-sync` | No | Remove the sync exclusion in the provider, then refresh |
| `sync-paused` | `pending-provider-sync` | No | Resume provider sync and refresh status |
| `remote-unavailable` | `pending-provider-sync` | No | Restore provider access and collect fresh evidence |
| `content-mismatch` | `pending-provider-sync` | No | Re-copy or reconcile the destination; do not evict the source |
| `unknown` | `pending-provider-sync` | No | Collect a complete, unambiguous provider observation |

The projection fixture is versioned with the API state vocabulary. For example,
`{ "provider_state": "content-mismatch", "goal_state": "pending-provider-sync", "eviction_permit": false }`
and `{ "provider_state": "complete", "goal_state": "provider-sync-confirmed", "eviction_permit": false }`
are mandatory negative and positive fixtures; the latter becomes `eviction-ready`
only when the independent permit gates pass.

### FR-3: Safe copy and eviction

- Native File Provider operations are bounded, re-hashed, and source-identity
  rechecked.
- A timeout or cancellation cleans only destination artifacts created by the
  current child process and writes a separate private failure journal containing
  candidate, source, destination, operation, bounded error code, and timestamp.
  `cloud-copy-cancelled` and every failed result are never successful receipts
  and can never authorize eviction; restart/readback must preserve that
  distinction.
- No provider placeholder or unmaterialized file is mutated. Existing-copy
  adoption must obtain provider-native `local-current`/`isDownloaded` evidence
  before any read or hash, and the status plus post-hash identity must be bound
  to the adoption receipt.
- Native and provider-API copy paths both require operation state, deadline, and
  cancellation checks before start, between chunks, and immediately after a
  successful write; cleanup/retention and source/eviction invariants are tested
  for timeout and cancellation.
- A successful native copy and a successful provider-API copy each write an
  immutable receipt, read that receipt back after the writer is gone, and verify
  receipt identity plus provider attestation before a permit can be returned.
- Eviction is disabled until the receipt, identity, and provider attestation
  are current and complete.

### FR-4: Reclaimable caches

Regenerable caches are a separate reclaim domain. Each proposal is identity
bound, active-use checked, journaled, reversible through OS Trash, and excluded
from user-data upload. The cache exclusion fixture must produce no cloud-copy
candidate, provider upload request, or cloud success receipt; only the local
cleanup journal and reversible Trash result may be created.

### FR-5: Explanations and lineage

The UI and export show stable content identifiers, provenance edges, confidence,
blockers, evidence timestamps, and the next user action without exposing raw
private paths. Dynamic Goal and ADR projections are views over receipts and never
authorize mutation.

### FR-6: Optional intelligence boundary

The deterministic Rust gate owns safety and arithmetic. A local model may rank
or explain a fixed maintenance command after dry-run evidence and explicit
confirmation. Noema, contextual-orchestrator, semantic-data-portal, or an
external LLM is optional integration code, never a runtime prerequisite for
standalone transfer or deletion.

## Non-functional requirements

- **Safety:** fail closed on missing, stale, contradictory, or provider-global
  evidence; no heuristic or arbitrary deletion weight.
- **Performance:** bounded child processes and asynchronous UI refreshes must
  keep the desktop responsive; every long operation has a visible timeout and
  cancellation path.
- **Privacy:** shareable projections, logs, receipts, and exports are path-free
  by default; the local `NaruonFileLineageEnvelope` is a protected,
  path-bearing compatibility envelope and must not be used as a shareable
  export. Provider raw output is never persisted. OAuth refresh tokens, when explicitly
  enabled, are stored only in the operating-system credential store and never
  in application logs, receipts, or exports.
- **Portability:** macOS, Linux, and Windows capability differences are explicit
  in release evidence; native File Provider behavior is never assumed on other
  platforms.
- **Auditability:** Rust safety decisions are deterministic, testable, and
  connected to ADR-0001/0002/0006/0007 and the exact-head baseline.

## Acceptance evidence

| Requirement | Required proof |
| --- | --- |
| FR-1 | Metadata precedence fixtures prove embedded → filename token → filesystem creation → modification fallback; missing, malformed, conflicting, provider-managed, and ambiguous states retain blockers and no eviction permit; the inventory receipt remains path-free |
| FR-2 | The provider-state table and projection fixtures above cover every `ProviderSyncState`; state-machine fixtures for `local-current + is_uploaded=false`, provider timeout, quota/auth uncertainty, stale or incomplete evidence, insufficient headroom, failed/pre-copy/cancelled observations, and sync completion prove the correct Goal transition while retaining blockers and no eviction permit until every gate is complete |
| FR-3 | Native receipt persistence/readback is covered by `verified_copy_keeps_source_and_writes_read_only_receipt`. Provider-API uploads share the active-operation cancellation command, retain one command-owned overall deadline across all chunks, check the bound before start and between chunks, and recheck immediately after provider success before writing a receipt. Cancellation or deadline failure abandons the resumable session; a post-success failure deletes the uploaded object; every provider upload failure is written to the private failure journal. Focused control tests cover pre-start, between-chunk, post-success cancellation, and overall deadline. A command-level mocked provider attestation and permit denial/approval test remains a tracked P1 gap. |
| FR-4 | The cache-exclusion fixture proves no cloud candidate, provider upload, or success receipt is produced; dry-run, identity, active-use, Trash, journal, and rollback tests cover the local reclaim lifecycle |
| FR-5 | UI/export fixtures prove stable identifiers, provenance, confidence, timestamps, blockers, and next-action wording; paths are redacted in shareable projections and Goal/ADR projections cannot authorize mutation |
| FR-6 | No external-service startup test and optional integration boundary tests |

## Explicit non-goals

- Automatic deletion based only on filename date, size, age, or an LLM score.
- Treating desktop-client free space as provider API quota.
- Force-killing `bird`, `fileproviderd`, Finder, or cloud clients.
- OAuth as a prerequisite for a single-user local-only installation.
- Uploading caches or mutating cloud placeholders.

## Traceability

- ADR-0001: provider evidence, metadata precedence, native copy, headroom, and
  eviction gates.
- ADR-0002: per-item cache cleanup and the narrow approval boundary.
- ADR-0003: local Zotero metadata handoff independent of cloud receipts.
- ADR-0006: bounded iCloud health evidence and timestamp comparison.
- ADR-0007: integrity-checked three-stream native-copy cohort.
- `docs/product-technical-gap-baseline.md`: exact-head product/technical gaps
  and live PR evidence.
