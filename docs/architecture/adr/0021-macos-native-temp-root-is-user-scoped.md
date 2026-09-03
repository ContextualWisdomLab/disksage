# ADR-0021: macOS native temp reclaim is user-scoped

**Status:** Proposed

**Supersedes:** ADR-0020 only for the macOS temporary-root selection rule. All evidence, approval,
identity, active-use, Trash, journal, and permanent-delete prohibitions in ADR-0020 remain in force.

## Context

ADR-0020 defines the evidence-bound native temporary-reclaim boundary and describes macOS `/tmp`
canonicalization to `/private/tmp`. The ontology-backed reclaim work in PR #334 narrows the current
macOS discovery root to `std::env::temp_dir()`, which is the temporary directory reported for the
current process rather than the global `/tmp` namespace. Keeping the broader root statement while
the active branch uses the per-user root would make the architecture record contradict the code and
the `macos_temp_reclaim_user_root` regression.

ADR-0012 through ADR-0019 remain reserved by the earlier active container/runtime-reclamation
owner lineage. This follow-on decision therefore uses ADR-0021 rather than reusing ADR-0013.

## Decision

While PR #334 remains unmerged, treat this decision as Proposed. The macOS adapter resolves the
OS-reported temporary root with `std::env::temp_dir()`, requires it to be absolute, and canonicalizes
that exact root before discovery. It does not broaden discovery to global `/tmp` merely because
`/tmp` canonicalizes to `/private/tmp`.

Candidate authority remains unchanged from ADR-0020: only marker-bound generated artifacts with a
complete bounded manifest, stable object identity, complete inactive-use evidence, a fresh exact
approval phrase, final identity/active-use revalidation, reversible OS Trash, and journal evidence
may cross the destructive boundary. Provider-managed roots, Photos libraries, symlink-like entries,
partial scans, timeouts, unknown data, and permanent deletion remain unavailable.

## Consequences

The macOS buyer-visible reclaim scope is intentionally narrower: DiskSage may leave reclaimable data
under unrelated global temporary namespaces untouched rather than infer ownership from location.
The rule follows the actual caller environment and therefore requires current-head macOS tests; it
must not be inferred from Linux `TMPDIR` fixtures or documentation alone.

If the protected implementation later adopts a different native API or root contract, a new ADR
must supersede this proposal rather than silently rewriting ADR-0020 or this record.

## Rejected alternatives

- Keep scanning `/tmp` globally on macOS: rejected because a global location is not sufficient
  ownership evidence for local-first destructive assistance.
- Reuse ADR-0013: rejected because ADR-0012 through ADR-0019 are already reserved by the active
  container/runtime-reclamation owner lineage.
- Retain ADR-0020 text while changing code only: rejected because shipped architecture must be
  reconstructable without branch archaeology.
- Treat the design spec as architecture authority: rejected; the spec and regression are evidence,
  while ADR status records the decision lifecycle.

## Evidence

- `src-tauri/src/temp_reclaim.rs::native_temp_root` uses `std::env::temp_dir()` and canonicalizes the
  returned absolute path.
- `src-tauri/tests/macos_temp_reclaim_user_root.rs` exercises the macOS user-root contract.
- `docs/superpowers/specs/2026-08-03-macos-user-temp-guard-design.md` records the originating design
  evidence but does not supersede this ADR lifecycle.
