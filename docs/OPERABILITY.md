# DiskSage Operability, Recovery, and Support Guide

## Operating posture

DiskSage is local-first. Core deterministic filesystem safety does not depend on an always-on network service. Provider and CWL integrations are optional failure domains that degrade independently.

## Capability discovery

The product distinguishes local deterministic capabilities, optional model availability/integrity, provider/native integration capability, optional CWL integration capability, and unsupported/incomplete state. An unavailable optional capability never grants fallback mutation authority.

## Degraded modes

### Model unavailable or invalid

Disable model-backed explanation that requires it. Keep deterministic features available where their prerequisites pass. Never silently load an unverified substitute artifact.

### Provider API unavailable

Remote state remains unknown. Local read-only evidence may continue; provider-dependent copy/eviction decisions fail closed where remote proof is required.

### Provider client/runtime not observed

Only the affected prerequisite is unavailable. Do not infer account logout, capacity, sync completion, or data loss.

### Naruon/contextual-orchestrator unavailable

Keep standalone operation. Cross-service advisory features report stable unavailable/degraded state without changing local authority.

### Evidence bound exceeded

Return explicit incomplete evidence. A truncated scan or response is never presented as complete success.

### Required receipt/private dossier cannot be created

Do not proceed when the operation contract requires safe durable evidence and its destination cannot be established.

## Diagnostics

Shareable diagnostics use bounded stable reason categories and exclude raw paths, secrets, provider response bodies, unrestricted command output, model bytes, and private account identifiers. Local developer diagnostics remain access-controlled and preserve enough category context to distinguish integrity, type, size, authorization, provider, resource, and transient transport failures.

## RCA and remediation

For unexpected behavior:

1. reproduce/refetch exact current evidence;
2. identify the first failing boundary;
3. distinguish symptom, immediate cause, root cause, and owner;
4. enumerate materially distinct remedies;
5. prove feasibility against permissions, tools/APIs, credentials, exact state, writer lease, blast radius, rollback, and acceptance evidence;
6. execute the smallest safe root-cause remedy;
7. rerun the exact failed path and full relevant verification;
8. use a failed/no-op attempt as new evidence rather than repeating it blindly.

Three materially distinct failed hypotheses across layers trigger architecture/governance reassessment instead of another symptom patch.

## Recovery model

Read-only retries use fresh evidence and never reuse a stale completeness claim. Mutations revalidate preconditions and recover only invocation-owned output or exact captured identity. Source material is preserved unless separately authorized.

After process termination or power loss, existence of an output path is not proof of completion. Re-observe source/destination/evidence and verify receipts/identity before offering recovery.

Provider acknowledgement/local copy does not imply remote durability; waiting/unknown state remains until the reviewed proof exists or the workflow explicitly documents a weaker result.

## Observability

Privacy-aware telemetry, where implemented, may record operation type, stable result code, elapsed duration, bounded item/byte counts, resource-limit refusals, recovery-required/completed outcomes, provider error/capability category without private scope, model integrity/evaluation category without bytes, and UI degraded state.

External telemetry is explicit and governed; private filesystem evidence is not a default payload.

## SLI/SLO posture

No numeric availability, latency, RPO, RTO, or capacity SLO is asserted without representative measurements. Candidate SLIs include bounded-scan success by workload class, unauthorized-mutation incidents, recovery completion, provider-evidence completeness, crash-free operation, memory/throughput by profile, and release rollback rehearsal success.

Numeric objectives require dated measurement evidence and are never invented from architecture prose.

## Incident handling

A product-security incident records exact affected release/source identity, user-visible impact, authority/data boundary, reproduction, root cause, mitigation, regression test, release/rollback decision, and disclosure path. Sensitive evidence uses private reporting channels.

## Software-delivery dependency failure

A central workflow, reviewer provider, or GitHub outage blocks only the dependent merge/release action. It does not authorize weaker local evidence and does not stop safe work on other branches/issues.

Repository automation uses a branch-local writer lease and exact current source/base identity before writes. Waiting lanes are deferred rather than polled indefinitely.

## Backup and retention

DiskSage owns no central server database today. Source data remains operator/provider-owned; restricted receipts/dossiers follow their explicit local location; source-controlled docs/model specs use Git; release/provenance artifacts follow release retention; future persistence must add explicit backup/restore/retention/deletion semantics.

## Upgrade and rollback

Upgrades preserve explicitly supported evidence/receipt formats or provide migration. Rollback cannot silently reinterpret newer authority records with weaker semantics. See `docs/RELEASE_AND_ROLLBACK.md`.

## Support evidence

Prefer stable reason codes, product version, operation type, bounded environment/profile context, and explicit user-consented diagnostics. Never ask users to post credentials, private paths, proprietary files, or full provider responses publicly.