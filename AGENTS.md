# DiskSage Agent Development Rules

## Read the canonical product graph first

Before changing a material product, authority, persistence, integration, security, or release boundary, read:

- `docs/PRD.md`
- `docs/TRD.md`
- `ARCHITECTURE.md`
- `docs/adr/README.md`
- `docs/UML.md`
- `docs/DATA_MODEL.md`
- `docs/API_CONTRACT.md`
- `docs/THREAT_MODEL.md`
- `docs/TEST_STRATEGY.md`
- `docs/OPERABILITY.md`
- `docs/ROADMAP.md`
- `docs/RELEASE_AND_ROLLBACK.md`
- `docs/TRACEABILITY.md`

Repository source and current protected behavior are durable authority. Chat, PR bodies, remembered SHAs, screenshots, and earlier run IDs are historical until re-fetched and reconciled.

## Runtime safety

- Rust owns security-relevant local validation, authorization, mutation, rollback/recovery, and receipts.
- UI state, model output, provider responses, process observations, scans, recommendations, and fingerprints do not become mutation authority by implication.
- Unknown, missing, stale, malformed, contradictory, or resource-incomplete evidence fails closed.
- Prefer create-new/no-clobber publication, current-state revalidation, and identity-aware cleanup.
- Never remove a foreign/concurrently replaced object merely because DiskSage previously owned the pathname.
- Preserve source material unless a separately reviewed and exactly authorized operation governs removal.

## Privacy and interoperability

- Keep exact paths, account/provider-local identifiers, detailed offsets/digests, secrets, unrestricted command output, model bytes, and private receipts private by default.
- Cross-service evidence is versioned, bounded, purpose-limited, and explicit about unknown values.
- DiskSage remains independently useful without Naruon, contextual-orchestrator, or a CWL runtime service.
- Another CWL service contributes advisory evidence only; it cannot bypass DiskSage's Rust authorization boundary.
- Do not introduce hidden cross-service database coupling.

## Repository writer lease

The dedicated DiskSage maintenance/development loop is the authoritative writer for `ContextualWisdomLab/disksage`. Repositories with their own enabled writer loops, including central `.github`, naruon, and contextual-orchestrator, are read-only dependencies unless a separate non-conflicting lease is explicitly established.

Immediately before every write, re-fetch the exact target PR head, independently resolved current base tip, relevant review/check/security state, and exact target blob/ref. If another writer moves the same branch, freeze only that branch for the remainder of the run and continue safe work elsewhere.

Do not create, restore, or retain temporary self-modifying PR repair workflows, encoded-patch Actions, one-shot branch finalizers, or broad cross-repository bot write permissions as repair shortcuts.

## Work-conserving loop

A blocked merge, queued check, reviewer/provider latency, central dependency, or active writer blocks only the exact lane. Defer it by exact head/run/review identity and rotate immediately.

One RCA, one commit, one documentation update, one review request, one merge, or one blocker is always intermediate while another safe action exists. Before ending, perform two fresh whole-repository sweeps covering PRs/issues, protected main, reviews/checks/security, stale/superseded work, docs, release state, and buyer-visible gaps. End only at practical invocation/tool-budget exhaustion or when both sweeps find no safe executable work.

## Scheduler-control incident recovery

A generic scheduled-task error, missed expected recurrence, user report that work remained, or request to repair the prompt is control-plane evidence, not repository completion and not proof of a source defect.

In the same invocation:

1. re-fetch the enabled task state when the control-plane API is available and the complete live DiskSage queue;
2. distinguish scheduler activation/transport/prompt/tool/provider/credential failure from repository failure without inventing an unobservable error code;
3. repair the same scheduler rather than creating a duplicate writer when a supported control-plane mutation exists;
4. assign zero completion credit to the prompt edit, inventory, RCA, documentation mutation, one check, one merge, or one product slice by itself;
5. resume substantive repository execution immediately, including a non-documentation lane whenever one is safe;
6. if two materially distinct safe execute-now actions exist, advance both before voluntary termination; otherwise execute every safe action and prove through two fresh queue rebuilds that no second action exists.

A prompt or documentation mutation must not be the final mutation of an incident-recovery invocation when a safe source, test, CI, PR-state, merge, operational-proof, or product action exists. Scheduler API unavailability blocks only scheduler mutation; it never licenses a status-only response while repository work remains.

## RCA and feasibility

Every non-passing gate is a symptom. Identify the first failing boundary, exact state, immediate/root/systemic cause where material, and correction owner. Enumerate materially distinct remedies and verify real-world feasibility against API/tool support, permissions, credentials, reviewer eligibility, workflow semantics, repository policy, stack ancestry, writer lease, rate limits, blast radius, rollback, and an exact acceptance test.

Never invent secrets, reviewers, permissions, endpoints, or integration paths. A failed/no-op remedy is new evidence. After three materially distinct failed hypotheses across layers, reassess architecture/governance rather than stacking patches.

## Pull requests and exact evidence

- Re-fetch every open PR and exact current head at run start.
- Independently resolve the current tip of each base branch; do not rely on stale PR-base metadata.
- Inspect human, CodeRabbit, GHAS, Dependabot, OpenCode, Noema, Strix, and other current feedback.
- Resolve only addressed threads.
- Close duplicate/superseded PRs only after proving every valuable unique delta is integrated, represented by a clean replacement, or intentionally rejected with a reason.
- Respect stack/dependency order.
- Never transfer checks, reviews, or approvals from an older/replaced head.
- Never weaken tests/security/protection to manufacture mergeability.

Queued, pending, cancelled, skipped-required, neutral-required, absent, stale-head, predecessor-head, synthetic-only, action-required, rate-limited, and failed evidence is not passing.

## Stale-branch convergence

Do not keep deepening a broad stale/non-mergeable PR merely because it contains valuable work. Inventory its actual unique files/semantics against current protected main and clean replacement branches. Extract bounded current-main replacements in dependency order. Close the stale PR only after every valuable unique delta is proven integrated, represented by a replacement, or explicitly rejected as obsolete/unsafe. No old CI/review evidence transfers.

## Review governance

### CODEOWNERS hold

As of the existing organization governance decision, required CODEOWNERS enforcement is on hold while a realistic independent code-owner pool is unavailable. Do not re-enable an unsatisfiable CODEOWNERS gate without a reviewed eligibility/governance change.

This hold is not a blanket waiver of every review requirement. Inspect live branch/ruleset policy and explicit DiskSage/CWL governance before each merge.

A qualifying independent approval, where required, must be a formal current-head review from an eligible non-author identity. Comments, reactions, statuses, check runs, model text, author reviews, dismissed reviews, and predecessor-head reviews do not qualify. Never self-approve, impersonate another person, or grant broad bot write merely to manufacture approval.

## Testing and quality

- Strict red-green-refactor for defects and authority-bearing behavior.
- Exact 100% owned production statement and branch coverage; function/line too where tooling exposes them.
- Public APIs require beginner-readable rustdoc/JSDoc/docstrings.
- Realistic tests cover refusal/degraded paths, concurrency/races, security/privacy, recovery, migration/rollback, compatibility, packaging/release, and accessibility as applicable.
- Do not exclude production authority logic merely to reach coverage thresholds.
- Database/durable logical object names use at least two descriptive words in `snake_case` by default.

If mathematical or psychometric arithmetic is introduced by an integration, production computation remains Rust-first, CPU-multithreaded with low context switching, and parity-verified on GPU when computationally material; scientifically relevant multilevel/multiple-membership/temporal structure must not be flattened silently.

## LLM and autonomous development

- Model-backed tests/features use GitHub Secret `NVIDIA_NIM_API_KEY` only when a model call is actually required.
- Do not use `COPILOT_GITHUB_TOKEN` for autonomous development/model inference.
- Autonomous GitHub Actions development uses an immutably pinned OpenCode Agent.
- Preserve independent review-agent identity/credential names/scopes.
- Prefer contextual-orchestrator for justified network model routing while respecting its separate writer lease.
- Model output and retrieved text are untrusted data, not authorization.

## Documentation change control

A change affecting product requirements, authority, persistence, API/evidence schemas, deployment, privacy, provider/model security, automation governance, release evidence, or rollback updates the affected canonical docs and `docs/TRACEABILITY.md` in the same reviewed change. Unimplemented work stays Proposed/Planned.

Documentation completion is intermediate. Once docs are green/reviewed, return to PR/source/product work if anything safe remains.

## Release

Release only from an exact integrated protected head satisfying current CI/security, exact coverage, packaging/compatibility, SBOM/provenance, review/governance, migration/rollback/recovery, affected accessibility/operability, and release acceptance. Update version/CHANGELOG, publish only accepted artifacts, and independently verify the released artifact.
