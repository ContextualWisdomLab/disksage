# DiskSage Repository Context

`AGENTS.md` is the governing development policy for this repository. Do not create a contradictory shadow policy here.

Before material work, read the canonical documentation index at `docs/README.md`, especially `docs/PRD.md`, `docs/TRD.md`, root `ARCHITECTURE.md`, `docs/adr/README.md`, `docs/DATA_MODEL.md`, `docs/UML.md`, `docs/THREAT_MODEL.md`, `docs/TEST_STRATEGY.md`, `docs/OPERABILITY.md`, `docs/ROADMAP.md`, `docs/RELEASE_AND_ROLLBACK.md`, and `docs/TRACEABILITY.md`.

Key invariants:

- DiskSage is local-first; Rust retains security-relevant filesystem authority.
- Observation/model/provider/repository evidence does not become runtime authorization by implication.
- Repository decisions bind the exact current source head and independently resolved live base tip.
- One branch-local writer lease prevents competing autonomous writes; waiting on one lane does not stop work elsewhere.
- Production quality targets exact owned coverage and beginner-readable public documentation.
- Model-backed autonomous development uses OpenCode plus `NVIDIA_NIM_API_KEY`, never `COPILOT_GITHUB_TOKEN` for model execution.
- Database/evidence names use descriptive two-or-more-word `snake_case` by default.
- Release requires exact integrated source, security/coverage/package/provenance/review/recovery acceptance and independently verified artifacts.

When source and documentation disagree, investigate current protected behavior and update the canonical docs/ADR rather than preserving contradictory prose.