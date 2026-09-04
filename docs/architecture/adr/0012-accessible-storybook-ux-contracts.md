# ADR-0012: Accessible Storybook UX contracts and design tokens

**Status:** Proposed  
**Date:** 2026-08-21  
**Figma File ID:** N/A — no Figma artifact was supplied for this slice; the token file and
Storybook scenes are the reviewable design source until a Figma handoff is approved.

## Context

DiskSage's desktop shell had repeated raw spacing, color, focus, and control styles spread across
Svelte components. The provider-stall incident also needs a stable, testable visual state for
`provider-sync-incomplete`, `materialization-stalled`, and `checking`, not a color-only warning.
The existing cloud and eviction authority must not be moved into the browser layer.

## Decision

1. Keep computation, provider evidence, and destructive authority in Rust and existing Tauri
   commands. The UI only renders state and emits bounded callbacks.
2. Adopt a three-level CSS token hierarchy (primitive → semantic → component) in
   `src/lib/ui/design-tokens.css`, with dark preference, forced-colors focus, reduced-motion, and
   44px control minimums.
3. Add `ProviderStatusCard` as a pure state renderer and maintain one Storybook story per clear,
   incomplete, stalled, checking, action, narrow-layout, and feedback edge state.
4. Run Storybook's accessibility addon in error mode against the built static output. The
   Chromium test runner must prove the cancel callback, disabled checking state, and 375px mobile
   viewport; stories do not call providers or mutate user files.
5. Keep Figma optional for this change because no approved Figma file exists. When a visual handoff
   is supplied, record its File ID in a superseding ADR and reconcile tokens before implementation.

## Consequences

- Every new customer-facing status can be reviewed at desktop and mobile widths before it is wired
  to a provider receipt.
- Keyboard, screen-reader, reduced-motion, dark-mode, and forced-colors behavior has one reusable
  contract instead of per-component guesses.
- Storybook and its dependencies increase development tooling size; they are dev-only and never
  become a runtime cloud or LLM dependency.
- Automated a11y is a first pass, not proof of complete WCAG conformance; VoiceOver, keyboard,
  zoom, and real File Provider states remain release acceptance work.

## Rejected alternatives

- Adding a UI framework solely for buttons/cards: existing Svelte and CSS custom properties cover
  the required surface with less runtime and dependency risk.
- Making the browser decide whether a provider is safe to evict: this would violate the existing
  receipt/identity/approval boundary.
- Treating Storybook green output as a substitute for hosted exact-head checks: stories only prove
  the rendered UI contract and event wiring.

## Standards and research basis (APA 7th)

World Wide Web Consortium. (2024, December 12). *Web Content Accessibility Guidelines (WCAG) 2.2*.
https://www.w3.org/TR/2024/REC-WCAG22-20241212/

World Wide Web Consortium. (n.d.). *ARIA Authoring Practices Guide*. Retrieved August 21, 2026,
from https://www.w3.org/WAI/ARIA/apg/

Design Tokens Community Group. (2025, October 28). *Design Tokens Format Module 2025.10*.
https://www.w3.org/community/reports/design-tokens/CG-FINAL-format-20251028/

Storybook. (n.d.). *Accessibility tests*. Retrieved August 21, 2026, from
https://storybook.js.org/docs/writing-tests/accessibility-testing

## Amendment: provider-indexing cancellation event (2026-08-25 11:31 +0900)

The provider status contract includes `provider-global-sync-indexing-pending` in the existing
bounded Finder-cancel event path. This keeps the Storybook event model aligned with the runtime
provider-global blocker set without granting the browser cloud-write or source-eviction authority.
The exact-head contract and Svelte checks pass at `b67ea3be`.

## Amendment: customer-action copy contract (2026-08-26)

Customer screens must describe the next safe action without exposing Rust command names, provider
internals, ontology identifiers, receipt/attestation fields, or implementation-only error text.
`cleanupCustomerCopyContract.test.ts` scans every Svelte customer surface, including dynamic
messages and static warnings, and requires bounded recovery guidance. Technical evidence remains
available to the backend and audit records; it is not rendered as customer instruction.
