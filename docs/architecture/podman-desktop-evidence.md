# ADR: Privacy-safe Podman desktop evidence

- **Status:** Proposed
- **Date:** 2026-08-05
- **Decision owners:** DiskSage maintainers
- **Related issue:** #107
- **Related headless contract:** #105 and `src-tauri/src/podman_reclaim.rs`

## Context

DiskSage already has a Rust-first, read-only Podman evidence probe that distinguishes VM configuration, raw-image logical size, host allocation, guest filesystem usage, Podman graph-root observations, and Podman-reported logical cleanup candidates. The desktop Cleanup experience previously had no supported way to inspect that evidence.

The UI must not turn evidence into authority. Podman documents that image reclaimable values can overstate what a prune would actually free when layers are shared. DiskSage therefore treats all `podman system df` candidate values as logical review evidence rather than verified host physical reclaimability.

The headless report also contains local-only details such as machine names, configuration paths, raw-image paths, graph-root paths, and dynamic command errors. Those details are useful for local diagnosis but are unnecessary for the desktop summary and unsafe for telemetry or shareable evidence. Tauri transport failures and arbitrary JavaScript rejection values can also contain account-local paths, socket names, or command detail, so the UI error boundary must redact them independently of the Rust projection.

## Decision

### 1. Add a separate privacy projection

`src-tauri/src/podman_desktop.rs` converts `PodmanReclaimPlan` into `PodmanDesktopEvidence`.

The projection includes only:

- configured machine disk bytes;
- raw-image logical bytes;
- host allocated bytes;
- guest total, used, and available bytes;
- Podman graph-root allocated and used bytes;
- image, stopped-container, and volume logical candidate bytes;
- unused-image and stopped-container counts;
- the SHA-256 commitment to the exact unused-image candidate set;
- evidence completeness, elapsed time, stable reason codes, and stable issue codes;
- separate image, stopped-container, and volume review boundaries;
- `physically_reclaimable_bytes`, which remains unknown until a before-and-after host observation proves it.

The projection excludes machine names and states; configuration, raw-image, and graph-root paths; image identifiers and tags; account-local context; command output and dynamic error details; and any mutation command or approval record.

Issue strings are reduced to the prefix before the first colon only when that prefix is a bounded lowercase kebab-case code: it must start with a lowercase ASCII letter, contain only lowercase ASCII letters, digits, or hyphens, and be no longer than 96 bytes. Delimiter-free paths, sockets, whitespace, uppercase text, Unicode, underscores, empty prefixes, and malformed values collapse to `podman-evidence-error`. Invalid candidate fingerprints fail closed: the fingerprint is removed, the evidence is marked incomplete, and a stable issue code is added.

A complete exact-image observation must contain both the exact unused-image record count and the SHA-256 commitment to that candidate set. The frontend rejects complete evidence when either member is missing and rejects a fingerprint that has no exact record observation. Partial evidence may retain safe exact-record counts after Rust removes an invalid fingerprint and emits an issue; this remains explicitly incomplete rather than being mislabeled as a complete candidate set.

Any projected issue code forces `evidence_complete` to false, even when an upstream caller incorrectly supplies `true`. The frontend independently rejects a response that combines `evidence_complete: true` with one or more issue codes. This keeps completeness as an integrity assertion rather than a cosmetic label.

The only assessment status admitted by schema version 1 is `unverified`. If a contradictory headless plan supplies a concrete `physically_reclaimable_bytes` value while the assessment remains unverified, the Rust projection clears that value before IPC, marks the evidence incomplete, and emits `podman-desktop-unverified-physical-reclaim-claim`. A future verified physical-reclaim contract requires an explicit schema and evidence-authority change; it cannot appear by silently forwarding a new headless value.

The two user-facing safety notices are also part of schema version 1 rather than arbitrary display text. The frontend accepts only those two exact statements in the defined order and count. Any modified, duplicated, reordered, additional, path-bearing, or otherwise noncanonical notice fails closed with `invalid-notices` instead of being rendered.

The platform field is also schema-bound because it appears in the user interface. Schema version 1 admits only the Tauri desktop targets `linux`, `macos`, and `windows`. Unsupported, path-bearing, machine-specific, or account-specific platform text fails closed with `invalid-platform` rather than becoming visible evidence.

### 2. Keep the Tauri command read-only and argv-based

`inspect_podman_reclaim` invokes the existing Rust probe using an executable plus an argument vector. It does not construct a shell string. The desktop surface exposes no prune, remove, machine start/stop, VM deletion, TRIM, raw-image mutation, or generic command execution path.

### 3. Keep review domains independent and conservative

Images, stopped containers, and local volumes have separate review booleans and separate UI sections. A review signal for one domain never authorizes another domain. This preserves future compatibility with distinct approval records and least-privilege workflows.

A positive candidate observation itself conservatively requires review in its own domain, even if an upstream assessment accidentally omits the corresponding recommended-action record. Rust derives the image, stopped-container, and volume review booleans from both the action list and the observed candidates. The frontend independently rejects a candidate domain whose required review boolean is false. An extra conservative `true` remains advisory only and never creates mutation authority.

### 4. Keep visual semantics explicit, accessible, and privacy-safe

The panel uses semantic headings, definition lists, buttons, `role="status"` for progress and results, and `role="alert"` for errors. The UI never uses color as the only carrier of completeness. Text labels always state whether evidence is complete or partial.

The UI never renders `String(reason)` or another untrusted exception representation. `podmanEvidenceErrorMessage` discards every transport, operating-system, and JavaScript failure detail and returns only `podman-evidence-unavailable`. Detailed diagnosis remains confined to trusted local logs and does not cross into the desktop evidence, telemetry, or shareable-evidence boundary.

### 5. Preserve standalone and MSA compatibility

The desktop response is a versioned JSON contract with no dependency on Naruon or another CWL service. DiskSage runs independently. A future Naruon or fleet-management adapter may consume the same privacy-safe schema without receiving local paths or identifiers.

## Consequences

### Positive

- Buyers can inspect a concrete Podman storage gap from the main Cleanup workflow.
- Logical size, host allocation, guest use, and verified physical reclaimability cannot be silently conflated.
- Contradictory unverified physical-reclaim claims are removed in Rust before IPC rather than relying on frontend refusal.
- Local identifiers stay outside the frontend contract, telemetry, and shareable evidence boundary.
- Malformed or delimiter-free probe issues cannot masquerade as safe codes or serialize local path content.
- Any issue forces partial evidence in Rust, and the frontend refuses contradictory complete-plus-issues payloads.
- Complete exact-image evidence cannot omit or detach its candidate-set commitment.
- Positive candidates cannot be displayed with a false no-review signal in their own domain.
- Arbitrary notice or platform text cannot become a path, machine-name, or account-detail display channel.
- Transport and JavaScript failures cannot leak machine names, paths, sockets, or command detail through the visible error region.
- The architecture can later add separate governed image, container, and volume approval records without changing the read-only evidence contract.
- Module-level `missing_docs` enforcement and source-level documentation contracts keep the Podman desktop functions beginner-readable.

### Negative

- The UI intentionally cannot perform cleanup. Operators must use a separate reviewed workflow until a mutation design includes exact candidate binding, independent approval, rollback evidence, and before-and-after host verification.
- Some evidence remains unavailable when Podman is absent, the machine is stopped, or the API is unhealthy. Unknown values remain `null`; the UI never converts missing evidence to zero.
- Visible failures intentionally use a stable generic code; sensitive operational detail must be inspected through trusted local diagnostics rather than the shareable desktop surface.
- Notice wording, supported platform identifiers, candidate/fingerprint relations, and review-boundary semantics are schema-bound; changing them requires coordinated Rust/frontend contract review rather than a copy-only UI edit.

## Verification matrix

| Invariant | Deterministic evidence |
|---|---|
| No machine names or paths in desktop JSON | Rust serialization tests search for private fixture values |
| Delimiter-free or malformed issue text cannot cross IPC | Rust unit and integration tests expect `podman-evidence-error` |
| Any projected issue forces partial evidence | `podman_desktop_issue_privacy.rs` contradicts upstream completeness and requires false |
| Complete-plus-issues payloads are rejected | TypeScript parser regression expects `inconsistent-evidence-completeness` |
| Complete exact-image evidence requires its fingerprint | TypeScript parser regression expects `inconsistent-image-candidate-fingerprint` |
| A fingerprint cannot exist without exact image records | TypeScript parser regression rejects detached commitments even for partial evidence |
| Observed candidates conservatively require domain review | `podman_desktop_candidate_review_consistency.rs` omits actions and requires all three review booleans |
| Candidate-plus-false-review payloads are rejected | TypeScript parser regressions cover image, stopped-container, and volume domains separately |
| Unverified physical-reclaim claims cannot cross IPC | `podman_desktop_physical_reclaim_claim.rs` requires removal, incomplete evidence, and a stable issue code |
| Arbitrary or duplicated notices cannot reach the UI | TypeScript parser regression requires the exact schema-v1 notice sequence |
| Unsupported or path-bearing platform values cannot reach the UI | TypeScript parser regression admits only `linux`, `macos`, and `windows` |
| Image/container/volume review separation | Rust projection tests and TypeScript view-model tests |
| Invalid fingerprint fails closed | Rust and TypeScript malformed-fingerprint tests |
| Missing observations stay unknown | Rust and TypeScript null-preservation tests |
| Exact Tauri command contract | Rust public-command integration test and mocked TypeScript invoke test |
| Schema/type/range drift rejected | TypeScript parser tests |
| Untrusted failure details never reach visible UI | `podmanEvidence.error.test.ts` supplies path, socket, object, null, and undefined failures and expects one stable code |
| Progress and errors announced | Svelte markup uses `role="status"` and `role="alert"` |
| No mutation surface | Registered command list exposes inspection only |
| Beginner-readable frontend function documentation | Source-level JSDoc regression test checks every production function declaration |
| Beginner-readable Rust function documentation | `missing_docs` plus `podman_desktop_documentation_contract.rs` |

## Release acceptance

This slice is release-eligible only after the exact integrated head passes Rust formatting and tests; frontend unit tests and exact coverage; Svelte type checking and production build; security and SAST workflows; current-head review with no unresolved actionable finding; actual repository/governance review policy; and packaging, provenance, and release acceptance.

## References

Podman. (n.d.). *podman-machine-inspect—Inspect one or more virtual machines*. Retrieved August 5, 2026, from https://docs.podman.io/en/stable/markdown/podman-machine-inspect.1.html

Podman. (n.d.). *podman-system-df—Show Podman disk usage*. Retrieved August 5, 2026, from https://docs.podman.io/en/latest/markdown/podman-system-df.1.html

Tauri Programme within The Commons Conservancy. (2026). *Calling Rust from the frontend*. https://v2.tauri.app/develop/calling-rust/

World Wide Web Consortium. (2024, December 12). *Web Content Accessibility Guidelines (WCAG) 2.2*. https://www.w3.org/TR/WCAG22/

World Wide Web Consortium. (2025). *Understanding Success Criterion 4.1.3: Status messages*. https://www.w3.org/WAI/WCAG22/Understanding/status-messages
