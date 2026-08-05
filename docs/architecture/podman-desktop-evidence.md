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

The projection excludes:

- machine names and states;
- configuration, raw-image, and graph-root paths;
- image identifiers and tags;
- account-local context;
- command output and dynamic error details;
- any mutation command or approval record.

Issue strings are reduced to the stable code before the first colon. Invalid candidate fingerprints fail closed: the fingerprint is removed, the evidence is marked incomplete, and a stable issue code is added.

### 2. Keep the Tauri command read-only and argv-based

`inspect_podman_reclaim` invokes the existing Rust probe using an executable plus an argument vector. It does not construct a shell string. Tauri documents commands as typed Rust functions registered once in `generate_handler!`; the desktop command follows that model and returns a serializable response.

The desktop surface exposes no prune, remove, machine start/stop, VM deletion, TRIM, raw-image mutation, or generic command execution path.

### 3. Keep review domains independent

Images, stopped containers, and local volumes have separate review booleans and separate UI sections. A review signal for one domain never authorizes another domain. This preserves future compatibility with distinct approval records and least-privilege workflows.

### 4. Keep visual semantics explicit, accessible, and privacy-safe

The panel uses semantic headings, definition lists, buttons, `role="status"` for progress and results, and `role="alert"` for errors. WCAG 2.2 requires status messages to be programmatically determinable without moving focus; the component uses live status regions for that purpose.

The UI never uses color as the only carrier of completeness. Text labels always state “증거 완전” or “부분 증거.”

The UI never renders `String(reason)` or another untrusted exception representation. `podmanEvidenceErrorMessage` discards every transport, operating-system, and JavaScript failure detail and returns only `podman-evidence-unavailable`. Detailed diagnosis remains confined to trusted local logs and does not cross into the desktop evidence, telemetry, or shareable-evidence boundary.

### 5. Preserve standalone and MSA compatibility

The desktop response is a versioned JSON contract with no dependency on Naruon or another CWL service. DiskSage runs independently. A future Naruon or fleet-management adapter may consume the same privacy-safe schema without receiving local paths or identifiers.

## Consequences

### Positive

- Buyers can inspect a concrete Podman storage gap from the main Cleanup workflow.
- Logical size, host allocation, guest use, and verified physical reclaimability cannot be silently conflated.
- Local identifiers stay outside the frontend contract, telemetry, and shareable evidence boundary.
- Transport and JavaScript failures cannot leak machine names, paths, sockets, or command detail through the visible error region.
- The architecture can later add separate governed image, container, and volume approval records without changing the read-only evidence contract.
- Headless API validation, error-redaction tests, and view-state tests remain deterministic and are included in the 100% frontend statement, branch, function, and line coverage gate.

### Negative

- The UI intentionally cannot perform cleanup. Operators must use a separate reviewed workflow until a mutation design includes exact candidate binding, independent approval, rollback evidence, and before-and-after host verification.
- Some evidence remains unavailable when Podman is absent, the machine is stopped, or the API is unhealthy. Unknown values remain `null`; the UI never converts missing evidence to zero.
- Visible failures intentionally use a stable generic code; sensitive operational detail must be inspected through trusted local diagnostics rather than the shareable desktop surface.

## Verification matrix

| Invariant | Deterministic evidence |
|---|---|
| No machine names or paths in desktop JSON | Rust serialization test searches for private fixture values |
| Image/container/volume review separation | Rust projection test and TypeScript view-model test |
| Invalid fingerprint fails closed | Rust and TypeScript malformed-fingerprint tests |
| Missing observations stay unknown | Rust and TypeScript null-preservation tests |
| Exact Tauri command contract | Mocked TypeScript invoke test |
| Schema/type/range drift rejected | TypeScript parser tests |
| Untrusted failure details never reach visible UI | `podmanEvidence.error.test.ts` supplies path, socket, object, null, and undefined failures and expects one stable code |
| Progress and errors announced | Svelte markup uses `role="status"` and `role="alert"` |
| No mutation surface | Registered command list exposes inspection only |
| Frontend logic coverage | `vitest.config.ts` includes `podmanEvidence.ts` and `podmanEvidenceError.ts` at 100% thresholds |
| Beginner-readable function documentation | Source-level JSDoc regression test checks every production function declaration |

## Release acceptance

This slice is release-eligible only after the exact integrated head passes:

1. Rust formatting and tests, including `podman_desktop` tests;
2. frontend unit tests and 100% coverage thresholds;
3. Svelte type checking and production build;
4. security and SAST workflows;
5. current-head review with no unresolved actionable finding;
6. independent non-author approval;
7. packaging, provenance, and release-acceptance workflows.

## References

Podman. (n.d.). *podman-machine-inspect—Inspect one or more virtual machines*. Retrieved August 5, 2026, from https://docs.podman.io/en/stable/markdown/podman-machine-inspect.1.html

Podman. (n.d.). *podman-system-df—Show Podman disk usage*. Retrieved August 5, 2026, from https://docs.podman.io/en/latest/markdown/podman-system-df.1.html

Tauri Programme within The Commons Conservancy. (2026). *Calling Rust from the frontend*. https://v2.tauri.app/develop/calling-rust/

World Wide Web Consortium. (2024, December 12). *Web Content Accessibility Guidelines (WCAG) 2.2*. https://www.w3.org/TR/WCAG22/

World Wide Web Consortium. (2025). *Understanding Success Criterion 4.1.3: Status messages*. https://www.w3.org/WAI/WCAG22/Understanding/status-messages
