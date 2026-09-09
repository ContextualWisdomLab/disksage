# ADR-0010: Require rooted, process-independent organize destinations

**Status:** Accepted
**Date:** 2026-08-25

## Context

Ontology `targetFolder` values are interpreted by the Rust organize planner. A relative value
would otherwise resolve against whichever process working directory launched DiskSage, and a
blind first-tilde replacement could rewrite a literal path segment such as `/opt/~archive`.
Either behavior can place a user file outside the reviewed destination and makes a plan
non-reproducible across desktop launchers.

## Decision

The planner accepts only an absolute target or an exact home token expanded against the supplied
absolute home: `~` and leading `~/` on every platform, plus native leading `~\` on Windows. It
rejects relative targets, named-user tildes, non-normal path components, and parent traversal.
Literal tildes inside absolute targets remain literal. The decision is enforced before a
`MovePlan` is emitted and is covered by Rust tests on the real planner path, including the
Windows-focused workflow contract.

## Consequences

- Invalid ontology destinations fail closed and produce no move plan.
- A launcher cannot change the meaning of a plan by changing its working directory.
- Existing absolute destinations and the metadata-first lineage boundary remain unchanged.
- Users must correct an invalid ontology target instead of receiving an implicit fallback.

## Rejected alternatives

- Canonicalizing a relative target against the current directory: this preserves ambient process
  authority and is not deterministic.
- Replacing every `~` token: this corrupts valid literal path segments.
- Silently rewriting invalid paths to the home directory: this invents a destination and weakens
  the review boundary.

## Evidence and standards

- OWASP Foundation. (2021). *Path traversal*. https://owasp.org/www-community/attacks/Path_Traversal
- MITRE. (2025). *CWE-22: Improper limitation of a pathname to a restricted directory*.
  https://cwe.mitre.org/data/definitions/22.html
