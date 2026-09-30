# DiskSage Incident, RCA, and Recovery Runbook

## Document status

**Status:** Proposed canonical incident-response baseline. This runbook covers DiskSage product failures and DiskSage-owned repository/release failures. Organization control-plane incidents owned by `ContextualWisdomLab/.github` are investigated here only far enough to classify and hand off the dependency without mutating that repository from the DiskSage writer lease.

## Incident goals

1. stop unsafe mutation or publication authority from expanding;
2. preserve source/user data and minimum necessary forensic evidence;
3. identify the first failing boundary rather than patching the last visible symptom;
4. execute the smallest feasible root-cause-changing remedy;
5. prove recovery on the exact affected product/repository boundary;
6. search for same-class recurrence before declaring closure.

## Severity dimensions

Severity is assessed independently across:

- data-loss or irreversible-mutation potential;
- unauthorized filesystem/provider/repository authority;
- secret/private-data exposure;
- integrity/provenance failure;
- availability/degraded-mode impact;
- release/update compromise;
- scope of affected users/artifacts/platforms;
- recoverability and evidence completeness.

A low-availability incident can still be high severity if it creates fail-open authority. A failed CI job is not automatically a product incident; classify the boundary first.

## Immediate containment

For runtime incidents:

- stop or refuse the affected mutation path if current safety evidence is incomplete;
- preserve source material and foreign/raced filesystem objects;
- do not retry with broader privileges or weaker validation;
- revoke/rotate affected credentials through their owning systems when exposure is plausible;
- isolate corrupt/untrusted artifacts without reclassifying them as safe;
- retain only the minimum private evidence required for recovery/forensics.

For repository/release incidents:

- do not merge, attest, publish, or release from stale/failed/ambiguous evidence;
- bind investigation to exact source head, independently resolved live base tip, run/attempt, workflow checkout, artifact digest, and relevant review/security evidence;
- treat central `.github` failures as read-only dependencies under the DiskSage writer lease;
- never weaken protection/tests or invent approval to restore flow.

## RCA contract

Every material failure investigation records:

1. **Observed symptom** — what failed or became unsafe.
2. **Exact evidence identity** — operation/plan/fingerprint or repository head/base/run/artifact identities.
3. **First failing boundary** — the earliest component that violated its contract.
4. **Immediate cause** — direct condition that triggered the failure.
5. **Technical root cause** — why the component could enter that condition.
6. **Systemic/control cause** — missing architecture, test, observability, ownership, review, or recovery control where material.
7. **Correction owner** — DiskSage, central CWL control plane, provider, runner, user environment, or external dependency.
8. **Falsifiable hypothesis** — what observation would prove or disprove the diagnosis.
9. **Detection gap** — why existing tests/telemetry/review did not catch it earlier, when applicable.

Do not convert infrastructure, reviewer, provider, permission, or policy failure into a fabricated source-code defect.

## Distinct remedies and feasibility

Before mutation, enumerate materially distinct remedies rather than variants of the same workaround. For each candidate verify:

- it changes the hypothesized root cause;
- the current actor/tool/API actually supports it;
- required credentials/permissions exist and are appropriately scoped;
- writer lease and repository policy permit it;
- reviewer/team/App eligibility is real when review is involved;
- dependency/stack order is correct;
- resource/runtime/rate limits are compatible;
- blast radius is bounded;
- rollback/recovery exists;
- security/privacy/coverage impact is acceptable;
- one observable acceptance test can prove the remedy.

Classify the candidate as `execute_now`, `defer_until_trigger`, `read_only_dependency`, `external_only`, or `reject`.

Prefer read-only inspection, compare, dry-run/no-op, permission probes, exact logs, and deterministic reproductions before an authority-bearing change.

## Test-first remediation

For a valid product/source defect:

1. establish the smallest realistic RED at the intended production boundary;
2. prove RED is caused by the defect, not broken setup/fixture/import;
3. implement the narrowest root-cause fix;
4. observe GREEN on the focused regression;
5. run the applicable complete suite and security/coverage checks;
6. re-fetch exact current head and live base/relevant runtime state;
7. resolve only review findings actually addressed.

For an operational/configuration defect where a code RED is inappropriate, establish an equivalent deterministic failing probe/contract before remediation.

A failed or no-op remedy is new RCA evidence. It is not a reason to stop while another safe distinct remedy or lane exists. After three materially distinct cross-layer hypotheses fail, reassess architecture/governance rather than stack a fourth symptom patch.

## Runtime recovery patterns

### Filesystem race or collision

Preserve the foreign replacement. Re-observe current identity, discard stale plan/approval, and require a new plan where mutation is still desired.

### Partial output

Use invocation-owned identity to clean only the output DiskSage can prove it created. Preserve source. If safe cleanup is not provable, enter explicit recovery-required state rather than deleting by pathname.

### Provider uncertainty

Keep remote/sync/capacity state unknown. Retry only the observation after the external cause changes; do not broaden local eviction/copy authority.

### Corrupt or mismatched model artifact

Reject installation/load. Remove only invocation-owned staging/output; preserve foreign destination replacements. Reacquire the reviewed artifact through the bounded integrity path.

### Private receipt/evidence failure

If the receipt is part of the mutation contract, fail closed before or roll back according to the feature-specific recovery rule. Do not silently continue without required evidence.

## Repository and CI recovery patterns

### Stale source or base evidence

Re-resolve exact PR head and live base tip. All predecessor-head checks/reviews/approvals are historical and cannot be transferred.

### CI infrastructure failure

Read the exact job/log and distinguish product failure from runner/network/action/bootstrap failure. Retry only a bounded classified transient failure. Do not edit product code solely to make an infrastructure outage disappear.

### Reviewer/rate-limit wait

Defer that exact lane once and rotate to other safe work. Do not perturb a clean head merely to retrigger a reviewer.

### Central control-plane defect

Document the first failing central boundary and hand it to the central `.github` owner. Do not duplicate central workflow logic into DiskSage to bypass the dependency.

### Release artifact/provenance mismatch

Stop publication. Rebuild from exact integrated protected source only after the mismatch root cause is fixed. Never re-label stale artifacts as current.

## Privacy incident handling

If secrets, raw paths, provider-local identifiers, private receipts, or unbounded payloads may have escaped their intended boundary:

- stop further export/logging of the affected field;
- identify exact recipients/artifacts/log surfaces;
- rotate/revoke secrets through the owner where relevant;
- remove exposure only through supported repository/provider mechanisms without destroying required evidence prematurely;
- record the minimal retained forensic data and access purpose;
- add a regression that proves the private/shareable boundary.

Do not paste sensitive evidence into public issue/PR comments to explain the incident.

## Recovery and closure evidence

An incident is closed only when the relevant exact boundary proves:

- root cause and owner are identified with supporting evidence;
- the selected remedy changes the root cause and passes its acceptance test;
- focused and applicable full validation pass on the exact repaired revision/runtime state;
- security/privacy/resource/recovery implications are rechecked;
- documentation/traceability are updated if the contract changed;
- same-class recurrence is searched in adjacent modules/workflows;
- protected-main or released-artifact operational proof exists when the incident affected integrated/release behavior;
- temporary repair machinery and stale workaround branches are removed or explicitly superseded.

A merged PR alone is not operational incident closure when the incident concerned protected-main scheduled/manual behavior or released artifacts.

## Work-conserving handoff

A blocked incident lane does not reserve the whole maintenance invocation. Record the deferment by exact identity, continue another safe PR/issue/product/documentation lane, and revisit after material state changes. Follow ADR-0006.

## Escalation boundary

Notify the operator only when a decision cannot be made safely under current authority: irreversible data-loss trade-off, legal/licensing authority, unavailable necessary credential/permission with no autonomous alternative, qualifying external approval when it is literally the sole substantive queue gate, or another safety/policy boundary.

See `docs/OPERABILITY.md`, `docs/DATA_GOVERNANCE.md`, `docs/THREAT_MODEL.md`, and `docs/TRACEABILITY.md`.