# Cloud review tenant-authority decision

## Status

Accepted for the cloud review queue and required for every approval decision evaluated by the frontend projection. This document records a local validation boundary; durable mutation authorization remains a trusted Rust responsibility and cannot be granted by this TypeScript module.

## Context

A cloud candidate carries two independent signals that an approval needs organization-tenant authority:

1. `destination_account_scope` identifies an organization destination.
2. `review_reasons` contains `organization-cloud-sensitive-context-needs-explicit-tenant-approval` when the candidate evidence requires explicit tenant review.

The previous predicate required both signals simultaneously. A missing, contradictory, stale, or malformed value in either field therefore made the approval path less restrictive. An approved decision with no tenant-authority attestation could become execution-ready even though the remaining signal still identified organization-sensitive handling.

This is an incorrect-authorization pattern: an authorization decision must not become more permissive because one of two security attributes is absent or contradictory. NIST SP 800-53 AC-3 requires access enforcement according to applicable policy, and OWASP ASVS 5.0.0 treats authorization as an independently verified security control. CWE-863 describes the broader weakness class in which an authorization check is performed incorrectly.

## Decision

`organizationTenantAuthorityRequired` uses fail-closed disjunction:

```text
organization destination scope
OR organization-sensitive tenant review reason
=> explicit organization-tenant authority attestation required
```

Either signal is sufficient. Only a candidate with neither signal follows the ordinary approval contract.

An approved decision is accepted only when its rationale starts with the exact backend-defined marker `[organization-tenant-authority-confirmed] ` whenever the predicate is true. Held decisions remain admissible without the marker because they grant no execution-ready approval. Candidate and decision fingerprints, reviewer attribution, rationale validation, and all durable Rust authorization checks remain mandatory and independent.

## Security invariants

- Missing or contradictory organization signals increase or preserve restrictions; they never reduce them.
- A candidate with organization scope but without the organization review reason still requires tenant authority.
- A candidate with the organization review reason but a non-organization scope still requires tenant authority.
- A candidate with neither signal does not receive an organization-only prompt.
- A valid attestation cannot replace exact candidate, review, destination, provider, account-scope, expiry, confirmation-phrase, or durable authorization binding.
- The frontend projection cannot mint, refresh, persist, or extend mutation authority.

## Test-first evidence

The regression test commit `6862a90593742843eea357b35d97cd89c8c07b7d` introduced mismatched-signal cases before the production predicate changed. The implementation commit `8f5cb741955b85cbbbdd43c090dbdef6b04c848b` changed the predicate from conjunction to disjunction and added beginner-readable JSDoc. The existing queue test was then aligned with the fail-closed contract in `ddbce4566718001ec297b288a9eb356300d3d382`.

The exact integrated head must still pass 100% statement, branch, function, and line coverage, the repository Test and Release workflows, CodeQL/SAST and Strix security gates, automated review, independent non-author approval, branch protection, and repository policy. Evidence from any earlier head is not reusable.

## Rollback

A rollback must revert the implementation, both regression-test commits, this decision record, and the matching changelog entry as one reviewed change. Reverting only the predicate would knowingly restore an authorization bypass and is prohibited. No database migration or persisted-schema rollback is involved.

## References

Joint Task Force. (2020). *Security and privacy controls for information systems and organizations* (NIST Special Publication 800-53 Rev. 5, updates through Release 5.2.0). National Institute of Standards and Technology. https://doi.org/10.6028/NIST.SP.800-53r5

MITRE. (2026). *CWE-863: Incorrect authorization* (CWE Version 4.20). https://cwe.mitre.org/data/definitions/863.html

OWASP Foundation. (2025). *OWASP Application Security Verification Standard* (Version 5.0.0). https://owasp.org/www-project-application-security-verification-standard/
