# ADR-0009 — Converge stale broad branches through clean current-base replacements

**Status:** Proposed governance decision.

## Context

Long-running branches can accumulate broad product, documentation, CI, and temporary repair changes while protected `main` continues to evolve. Repeatedly rebasing, deepening, or force-rewriting such a branch makes review evidence stale, obscures which changes remain valuable, and can preserve obsolete repair mechanisms merely because they share ancestry with useful work.

DiskSage currently has stale broad work whose valuable concerns are being reconstructed as bounded current-main replacements. The repository needs a durable rule for proving supersession without losing unique work or transferring stale evidence.

## Drivers

- preserve every valuable unique semantic delta;
- keep replacement work reviewable and based on current protected source;
- prevent `behind_by` or a newer base from being misread as proof of integration;
- avoid force-push/destructive rebase and predecessor-evidence transfer;
- eliminate obsolete one-shot/self-modifying repair machinery;
- keep exactly one active owner for each overlapping product/documentation concern.

## Alternatives considered

1. Keep deepening the stale branch until it merges — rejected because the diff and evidence surface continue to expand while the base moves.
2. Force-rebase the stale branch onto current main — rejected because it rewrites evidence identity and increases conflict/review risk.
3. Close stale branches as soon as newer replacements exist — rejected because replacement existence does not prove every unique valuable delta was preserved.
4. Decompose unique work into clean current-base replacements and close only after explicit convergence proof — selected.

## Decision

For every stale broad or stacked branch:

1. independently resolve exact current protected `main` and exact stale head;
2. compare current main -> stale head and enumerate every changed file plus unique semantic behavior;
3. classify each valuable delta as:
   - `integrated_on_protected_main`;
   - `preserved_on_clean_replacement`;
   - `explicitly_rejected_or_superseded` with technical reason;
   - `unresolved`;
4. create or reuse the smallest current-base replacement for unresolved valuable work, avoiding overlap with an existing canonical owner;
5. reacquire all checks, reviews, approvals, and release evidence on the replacement exact head; no predecessor evidence transfers;
6. close the stale branch only when no valuable delta remains `unresolved`.

`behind_by`, a newer protected-main commit date, a green predecessor check, or a replacement PR title is never sufficient supersession evidence.

One canonical documentation branch owns the documentation graph. One canonical implementation branch owns each overlapping source/control-plane concern. New work must not deepen the stale broad branch solely to make it appear current.

## Consequences

The open PR count can temporarily increase while clean replacements are established, but each branch has a bounded purpose and current base. Closure takes more evidence than a simple duplicate label, while future review and merge risk is reduced.

## Failure and recovery

If a replacement omits a unique valuable delta, mark convergence incomplete and either extend the correct current-base owner or create the smallest missing replacement. Do not reopen obsolete repair machinery unless a new accepted architecture explicitly requires it.

If protected main advances, re-evaluate ancestry and the replacement diff. Do not transfer predecessor checks/reviews.

## Security/governance impact

This decision reduces stale code, hidden writer authority, force rewriting, and false reuse of historical evidence. Security fixes are preserved by semantic comparison rather than branch age. Temporary write-capable repair workflows are explicitly not preserved merely for historical parity.

## Verification/acceptance

A stale PR closure record must identify the protected-main comparison, clean replacements/integrated commits that own each valuable concern, any explicitly rejected delta and reason, and the absence of unresolved unique work. Where practical, machine tests and traceability reference the replacement rather than stale PR prose.

## Migration/rollback

Existing stale branches remain open until convergence proof is complete. If a premature closure is discovered, restore the missing semantic change on a current-base branch; do not reconstruct stale evidence as current.

## Supersession

Supersede if GitHub or the organization provides a stronger transactional branch-decomposition/evidence-transfer mechanism that can prove semantic preservation without stale authority.