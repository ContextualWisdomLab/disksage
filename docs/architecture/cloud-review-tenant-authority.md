# Cloud review tenant-authority decision

## Status

Accepted for the cloud review queue and durable Rust transfer gate. This document records both the frontend projection and the trusted mutation boundary; the frontend remains incapable of granting durable mutation authorization.

## Context

A cloud candidate carries two independent signals that an approval needs organization-tenant authority:

1. `destination_account_scope` identifies an organization destination.
2. `review_reasons` contains `organization-cloud-sensitive-context-needs-explicit-tenant-approval` when the candidate evidence requires explicit tenant review.

The previous predicate required both signals simultaneously. A missing, contradictory, stale, or malformed value in either field therefore made the approval path less restrictive. An approved decision with no tenant-authority attestation could become execution-ready even though the remaining signal still identified organization-sensitive handling. A candidate whose ordinary `requires_review` flag was false could also bypass the tenant-authority requirement entirely.

This is an incorrect-authorization pattern: an authorization decision must not become more permissive because one of two security attributes is absent or contradictory. NIST SP 800-53 AC-3 requires access enforcement according to applicable policy, OWASP ASVS 5.0.0 treats authorization as an independently verified security control, and CWE-863 describes the broader weakness class in which an authorization check is performed incorrectly.

## Decision

Both TypeScript review projection and Rust transfer authorization use fail-closed disjunction:

```text
organization destination scope
OR organization-sensitive tenant review reason
=> explicit organization-tenant authority attestation required
```

Either signal is sufficient. Only a candidate with neither signal follows the ordinary approval contract.

An approved decision is accepted only when its rationale starts with the exact backend-defined marker `[organization-tenant-authority-confirmed]` followed by exactly one U+0020 ASCII space whenever the predicate is true. Held decisions remain admissible without the marker because they grant no execution-ready approval. If either organization signal is present while `requires_review` is false, both frontend and Rust fail closed instead of treating the candidate as ready. Candidate and decision fingerprints, reviewer attribution, rationale validation, copy-approval freshness, exact confirmation phrase, provider/account scope, and all other durable Rust authorization checks remain mandatory and independent.

## Security invariants

- Missing or contradictory organization signals increase or preserve restrictions; they never reduce them.
- A candidate with organization scope but without the organization review reason still requires tenant authority.
- A candidate with the organization review reason but a non-organization scope still requires tenant authority.
- Organization-sensitive evidence cannot bypass tenant authority merely because `requires_review` is false.
- A candidate with neither organization signal does not receive an organization-only prompt or blocker.
- A valid tenant attestation cannot replace exact candidate, review, destination, provider, account-scope, expiry, confirmation-phrase, or durable authorization binding.
- The frontend projection cannot mint, refresh, persist, or extend mutation authority.

## Test-first evidence

The TypeScript RED commit `76960c1db7707cfe402abd3d96409d3bf8baf0b6` introduced scope-only, reason-only, missing-attestation, and `requires_review = false` regressions before production changed. The Rust RED commit `1788a8e43cac3fea45daa05e6e6e8fde6e3841f8` exercised the public durable transfer gate for the same signal matrix. The production GREEN commit `be3a222a62685f22007eb097c0a86d7e4592cdb9` applies the disjunctive requirement in both frontend and Rust and blocks organization-sensitive candidates without an ordinary review flag. The follow-up `0f411d173643a3e8a727745599cb23bd9d020ef0` aligns an existing organization-scoped Naruon lineage fixture with the stricter attestation contract rather than weakening the gate.

No predecessor-head CI, review, or approval evidence authorizes these commits. The unchanged exact head must independently pass repository Test and Release workflows, current security/SAST gates, exact production coverage, actionable review closure, branch/ruleset policy, and any qualifying independent approval required by live policy or explicit governance.

## Rollback

Rollback is a reviewed security-boundary change. Reverting only the disjunctive predicate or the no-ordinary-review blocker would knowingly restore the fail-open condition and is prohibited. A justified rollback must revert the production behavior, both regression suites, this decision record, and the matching changelog evidence together, and must introduce an independently reviewed replacement authorization contract that remains at least as restrictive. No database migration or persisted-schema rollback is involved.

## Standalone and CWL integration boundary

The tenant-authority gate is local to DiskSage's review and transfer authorization. It does not require Naruon, contextual-orchestrator, or a central CWL runtime to function. Naruon may consume bounded lineage or readiness evidence, but it cannot manufacture the tenant attestation or bypass DiskSage's exact Rust transfer checks. Central organization workflows may verify the implementation as repository evidence; they do not become runtime authorization.

## References

Joint Task Force. (2020). *Security and privacy controls for information systems and organizations* (NIST Special Publication 800-53 Rev. 5). National Institute of Standards and Technology. https://doi.org/10.6028/NIST.SP.800-53r5

National Institute of Standards and Technology. (2025, August 27). *NIST releases revision to SP 800-53 controls*. https://csrc.nist.gov/News/2025/nist-releases-revision-to-sp-800-53-controls

MITRE. (2026). *CWE-863: Incorrect authorization* (CWE Version 4.20). https://cwe.mitre.org/data/definitions/863.html

OWASP Foundation. (2025). *OWASP Application Security Verification Standard* (Version 5.0.0). https://owasp.org/www-project-application-security-verification-standard/
