# DiskSage

[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/ContextualWisdomLab/disksage)

**Privacy-first desktop storage analysis and reclaim decision support for macOS, Windows, and Linux.**

DiskSage helps people under disk pressure understand what is consuming local storage, identify evidence-backed recovery candidates, and see the next safe action without trusting an unexplained deletion rule or model verdict.

It is local-first by design. Scanning and deterministic safety decisions do not require a cloud service, OAuth account, or external LLM. Optional provider and ecosystem integrations add evidence or convenience without becoming deletion authority.

## What DiskSage helps with

| Need | DiskSage approach |
| --- | --- |
| Understand disk use | Bounded, cancellable inventory with logical size, physical allocation, unknown regions, and category views |
| Reclaim regenerable space | Domain-specific review of caches, temporary data, and development artifacts rather than age-only deletion rules |
| Review duplicates | Exact-content evidence first; preservation and keeper decisions remain explicit |
| Handle stale development data | Evidence-aware Git, build-artifact, container, and VM workflows that preserve active or ambiguous state |
| Archive local data to cloud roots | Verify exact local copy and provider evidence before a separate local-source decision |
| Explain blocked actions | Show an observable condition and next safe action instead of exposing internal error/module names |
| Keep recovery evidence | Distinguish proposed bytes, processed bytes, and observed filesystem recovery in bounded receipts |
| Get optional advice | On-device model and ontology assistance may explain or classify candidates but cannot override deterministic safety gates |

## Safety is the product boundary

DiskSage does not treat “old”, “large”, “looks duplicated”, or “the model said so” as deletion authority.

A candidate becomes actionable only when the current domain can prove enough of the relevant identity, content, allocation, provider, active-use, lineage, and approval evidence. Missing, stale, conflicting, timed-out, or ambiguous evidence fails closed.

Important product invariants include:

- inventory and review are read-only until a separate action is explicitly approved;
- planning, copy, provider confirmation, local-removal review, and execution remain distinct states;
- source identity and evidence are rechecked immediately before mutation;
- symlinks, provider placeholders, active files, incomplete scans, and replacement races are preserved or rejected;
- user-file removal uses a reversible Trash/quarantine path with journal/receipt evidence;
- permanent deletion is not a user-file product action;
- a verified copy inside a cloud-provider root is not proof that the provider completed remote synchronization; and
- model or ontology output may advise, never self-authorize a filesystem mutation.

The canonical product contract is [`docs/PRD.md`](docs/PRD.md). Current implementation gaps and active evidence are tracked separately in [`docs/product-technical-gap-baseline.md`](docs/product-technical-gap-baseline.md).

## Current maturity

DiskSage is in **early development**. The source package metadata is currently `0.1.0`, and this repository has **no published GitHub release yet**. Source version numbers, successful local tests, open pull requests, or measured development experiments are not release or production-support claims.

The current source already contains substantial inventory, local reclaim, duplicate, cloud-evidence, provider-safety, Git/container/VM, recovery, and operational-audit foundations. Capability is intentionally platform- and provider-specific: when DiskSage cannot prove a safe operation on the current platform, that operation remains unavailable rather than being simulated.

The product-level outcome target is **300 GB of verified, attributable local capacity recovery on an eligible real-world workload**. That is a target, not a claim that every device has 300 GB available to reclaim or that DiskSage has already recovered that amount on a production device. Candidate logical bytes and observed filesystem free-space deltas remain separate measurements.

## Cloud-provider boundary

DiskSage can work with local roots exposed by iCloud Drive, OneDrive, and Google Drive, but the providers remain authoritative for their own account, quota, synchronization, and remote-object state.

The intended workflow is:

```text
reviewed candidate
      |
      v
exact local copy verified
      |
      v
provider sync pending
      |
      v
provider sync confirmed
      |
      v
local-source review
      |
      v
reversible local action
```

A local copy never silently becomes “uploaded”. A provider timeout, ambiguous account, stale capacity evidence, collision, placeholder, active use, or incomplete sync evidence keeps the local source in place.

Current provider capabilities are not assumed to be symmetric. In particular, native removal of only a local iCloud materialization is a distinct capability; DiskSage does not invent equivalent OneDrive or Google Drive authority where it cannot prove one.

## Quick start for source development

DiskSage is a Tauri 2 / Rust / Svelte 5 desktop application. Current source metadata requires Node.js `^20.19.0 || >=22.12.0`; the Tauri crate currently declares Rust `1.88` as its minimum compiler baseline on this documentation branch.

Install the JavaScript dependencies from the lockfile:

```bash
npm ci
```

Run the desktop development application:

```bash
npm run tauri -- dev
```

Frontend-only development is also available when working on non-native UI surfaces:

```bash
npm run dev
```

Because there is no published DiskSage release yet, these commands are source-development paths, not an end-user installer promise.

## Verify the source

Run the repository's ordinary frontend checks:

```bash
npm run check
npm test
npm run build
```

For the Rust/Tauri source:

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Some specialized operational CLIs are feature-gated and platform-specific. Use the applicable command documented by its architecture/runbook rather than enabling a mutation because a binary happens to compile.

A passing source suite is engineering evidence for that exact revision. It is not proof of a published release, provider synchronization, physical-capacity recovery, or safe eligibility for a particular user's file.

## Architecture at a glance

```text
Local filesystems / provider roots
              |
              | bounded read-only evidence
              v
+------------------------------------+
|              DiskSage              |
|------------------------------------|
| inventory + physical allocation    |
| evidence / identity / active use   |
| domain-specific reclaim planning   |
| optional local model + ontology    |
| approval + revalidation boundary   |
| reversible action + receipts       |
+------------------+-----------------+
                   |
          explicit reviewed action
                   |
                   v
          OS / provider boundary
```

DiskSage owns local storage analysis, evidence-backed reclaim decisions, reversible local-action contracts, and their receipts. It does not own cloud-provider synchronization truth, customer accounts, external filesystem semantics, another ContextualWisdomLab product's data model, or a general-purpose remote execution plane.

Optional ecosystem consumers and providers integrate through explicit contracts. They do not gain shared filesystem, database, credential, or deletion authority merely because DiskSage can exchange evidence with them.

## Privacy and network posture

DiskSage is local-first. File content, raw private paths, account identifiers, provider database rows, command lines, and secrets do not leave the device by default.

Private receipts and detailed evidence stay local with restrictive storage boundaries. Shareable diagnostics should use bounded aggregates, stable codes, timestamps, and fingerprints rather than customer paths or credentials. Optional network integrations require an explicit purpose and documented data boundary.

The on-device advice path is intentionally separate from deterministic mutation eligibility: advice can explain why an item may be interesting, but it cannot make an unsafe or insufficiently evidenced action executable.

## Customer-visible states

DiskSage's customer copy should help a person decide what to do next:

| State | Meaning | Next action |
| --- | --- | --- |
| Scanning | Evidence collection is still running | Keep DiskSage open or cancel safely |
| Review ready | Exact candidates and consequences are available | Review selected items and recovery method |
| Waiting for provider | A cloud copy is not yet proven current remotely | Let the provider finish, refresh, and keep the local source |
| Needs attention | Capacity, activity, permission, collision, or identity evidence is incomplete | Resolve the named condition and scan again |
| Approved, rechecking | DiskSage is validating the exact reviewed state again | Avoid changing the selected items until the check finishes |
| Recovered | The reversible action completed and a receipt exists | Verify the measured result; use Undo/Restore if needed |
| No eligible candidates | No further safe action is currently supported | Review preserved blockers or choose another scan domain |

Internal module names and raw provider diagnostics belong in developer/operator evidence, not in customer-facing explanations.

## Documentation map

- [`docs/PRD.md`](docs/PRD.md) — canonical product outcomes, jobs, safety invariants, provider capabilities, and acceptance criteria.
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — architecture and ownership index.
- [`docs/architecture/adr/`](docs/architecture/adr/) — accepted architecture decisions and safety boundaries.
- [`docs/product-technical-gap-baseline.md`](docs/product-technical-gap-baseline.md) — dated implementation and product-gap evidence.
- [`docs/superpowers/specs/`](docs/superpowers/specs/) — detailed accepted designs where deeper implementation context is needed.
- [`CHANGELOG.md`](CHANGELOG.md) — integrated user-visible source changes; not release evidence by itself.
- [`SECURITY.md`](SECURITY.md) — security policy where present in the current checkout.

## Contributing

Start with [`AGENTS.md`](AGENTS.md), the canonical PRD, architecture/ADR records, and the current gap baseline before changing a safety-sensitive workflow.

Keep changes inside DiskSage's local storage-analysis and reclaim responsibility. New cleanup or provider capabilities need evidence specific to that domain; do not convert a rule of thumb, filename date, model score, external tool output, or sibling-service response into mutation authority. Customer-facing changes should state the observable condition and next safe action rather than expose implementation ownership.

## License

DiskSage is licensed under the [MIT License](LICENSE). Third-party crates, JavaScript packages, provider APIs, datasets, models, fonts, and other external assets retain their own terms and are not relicensed by DiskSage.
