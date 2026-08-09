# DiskSage Repository Context

This file is a concise navigation aid for coding agents. It does not override `AGENTS.md`, repository policy, branch protection, or the canonical product/architecture documents.

## Read first

1. `AGENTS.md` — development and repository-authority rules.
2. `docs/PRD.md` — product outcomes, users, non-goals, and acceptance.
3. `docs/TRD.md` — technical and evidence contracts.
4. `ARCHITECTURE.md` — system context, trust, authority, deployment, privacy, release evidence.
5. `docs/adr/README.md` — material decisions and status.
6. `docs/UML.md` — component, sequence, state, deployment, and repository-authority diagrams.
7. `docs/DATA_MODEL.md` — conceptual versus persisted evidence model and ERD.
8. `docs/THREAT_MODEL.md`, `docs/TEST_STRATEGY.md`, `docs/OPERABILITY.md`, and `docs/TRACEABILITY.md`.
9. `CHANGELOG.md` before any release-affecting change.

## Non-negotiable engineering boundaries

- DiskSage remains independently operable; CWL services are optional integrations.
- Rust owns security-relevant local validation, authorization, mutation, rollback/recovery, and receipts.
- Observation, model output, provider response, UI state, CI status, or a Git reference does not become runtime mutation authority.
- Missing, unknown, stale, malformed, contradictory, or out-of-bound evidence fails closed.
- Preserve the private-versus-shareable evidence boundary; do not leak raw paths, secrets, provider-local identifiers, model bytes, or unrestricted diagnostics into public contracts.
- Prefer no-clobber/create-new and identity-aware cleanup. Never delete a foreign object because a pathname was raced.
- Database/evidence object names use at least two descriptive words and `snake_case` unless an external ecosystem requires another convention.
- Public APIs require beginner-readable documentation; owned production statement/branch/function/line coverage targets 100% where tooling exposes the dimensions.

## Repository evidence

Before a write or merge decision, re-fetch the exact current source head, independently resolved live base tip, relevant reviews/threads/checks/status/security state, and exact target blob/ref. Historical PR descriptions, remembered SHAs, predecessor checks, comments, reactions, rate-limit messages, or synthetic merge evidence are not current authorization.

Do not self-approve or manufacture approval. Do not weaken checks, branch protections, tests, security gates, or coverage to make a PR mergeable. Waiting on one check/reviewer blocks only that action; continue safe non-conflicting work.

## Automation

The dedicated DiskSage writer loop owns writes to this repository. Repositories with their own writer loops, including central `.github`, naruon, and contextual-orchestrator, are read-only dependencies unless a separate safe lease is established.

Do not create or revive self-modifying PR repair workflows, encoded-patch Actions, one-shot finalizers, or broad bot permissions as a repair shortcut. Model-backed autonomous development uses `NVIDIA_NIM_API_KEY` through GitHub Secrets and an immutably pinned OpenCode Agent; do not use `COPILOT_GITHUB_TOKEN` for that purpose.

## Documentation change control

A change to trust, authority, persistence, integration schemas, model/provider security, release evidence, or operational recovery updates the relevant canonical document/ADR and `docs/TRACEABILITY.md` in the same PR. Keep active proposals labeled as active/planned until protected integration proves them.