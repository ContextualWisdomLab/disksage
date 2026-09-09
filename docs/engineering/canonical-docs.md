# Canonical docs index (repo-factual, 2026-09-09)

The build prompt names `docs/engineering/execution-policy.md` and sibling
canonical docs as authoritative, but most of them do not exist in this repo.
This index records what exists vs what is absent, without inventing policy.

## Present in this repo

- `docs/product-technical-gap-baseline.md` — dated product/technical gap
  baseline; its "Loop update rule" governs dated-evidence appends.
- `docs/architecture/adr/` (ADR-0001..0011), `docs/architecture/goals/`,
  `docs/architecture/cloud-review-tenant-authority.md`
- `docs/development/` — operator runbooks (cache cleanup, cloud offload,
  iCloud eviction batch, Zotero local API + reference manifest)
- `docs/doctoring/` — release/model artifact integrity and provenance notes
- `docs/superpowers/plans/`, `docs/superpowers/specs/` — dated plans/specs
- `docs/maven-cache-audit.md`
- Root: `AGENTS.md`, `README.md` (safety contract), `CHANGELOG.md`,
  `SECURITY.md`

## Absent from this repo

`docs/engineering/` previously did not exist, so none of the following exist:
`execution-policy.md`, `acceptance-criteria.md`, `review-policy.md`,
`coding-rules.md`, `skills-subagents-mcp.md`, `runtime-data-policy.md`,
`harness-engineering.md`, `token-cost-visibility.md`. Likewise absent:
`docs/workflow/pr-continuity.md`, `docs/workflow/one-day-delivery-plan.md`,
`docs/operations/deploy-runbook.md`,
`docs/operations/manual-publishing-runbook.md`, `prompts/build.txt`,
`prompts/plan.txt`, `ARCHITECTURE.md`.

## De-facto authorities used by the 2026-09-09 loop

1. This index (existence facts only, no invented rules).
2. The gap-baseline loop rule (dated evidence only; no transfer/deletion
   authority from incomplete probes, comments, or reviews).
3. The `README.md` safety contract.
4. The GitHub ruleset `CWL Central required workflows` on `main`
   (`opencode` / `noema` / `strix` / `codeql-pr`).

Do not create the absent policy files with invented rules; record a real
repo decision first, then document it.
