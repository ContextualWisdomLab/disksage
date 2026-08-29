# DiskSage UI/UX and Storybook event inventory

**Status:** In progress, exact source head `feat/storybook-ux-contracts`  
**Scope:** desktop Svelte shell and provider-status feedback states  
**Visual source:** no Figma file was supplied for this product slice; the code token file is the
reviewable source of truth until a Figma handoff exists.

This inventory turns customer-visible states into repeatable Storybook scenes. It is not cloud or
eviction authority: every destructive action remains behind the Rust evidence and approval gates.

## Story and event matrix

| Story | Trigger/event | Expected customer action | Accessibility and edge assertion |
| --- | --- | --- | --- |
| `Clear` | Provider observation is complete and quiet | Continue to per-file review; do not assume eviction | Status is announced politely; no destructive action is shown |
| `IncompleteEvidence` | Cloud status is missing or stale | Check the cloud app and connection, then recheck | State is text, not color alone; last-check time is visible |
| `MaterializationStalled` | File preparation has stopped | Cancel the Finder copy, then recheck; do not retry immediately | Cancel button has an accessible name and invokes one bounded callback |
| `CheckingWithoutAction` | A read-only provider probe is running | Wait; do not cancel an operation that has no cancel authority | Action is disabled and exposes `aria-disabled` |
| Scan start | Scan button activates | Review progress and wait for completion | Root is labelled; unavailable roots disable the action |
| Scan failure | IPC/start or post-scan load fails | Read the error and retry | `role=alert` presents the next action; no `alert()` steals focus |
| Navigation | Breadcrumb or directory button activates | Move to the selected directory | Landmark and button names are keyboard reachable |
| Reduced motion | `prefers-reduced-motion: reduce` is enabled | Use the same controls without animation | Global token contract disables transitions/animations |
| Narrow viewport | 375px viewport | Scroll and operate controls without horizontal clipping | Controls become full width and retain 44px touch targets |

## Required review dimensions

- **Accessibility:** WCAG 2.2 AA target; semantic headings, labels, skip link, focus-visible ring,
  live regions, keyboard operation, non-color status text, and forced-colors support.
- **Touch & interaction:** controls use the shared minimum size token; every async action has a
  disabled/loading state and a bounded, reversible next action.
- **Performance:** the shell does not poll during a scan; provider polling/backoff remains in the
  existing CloudArchive state machine; CSS uses no new runtime animation or layout library.
- **Style selection:** primitive, semantic, and component tokens live in one CSS contract; raw
  colors are not introduced in the new shell paths.
- **Layout & responsive:** mobile-first wrapping is tested by Storybook's mobile viewport and the
  shell has a readable max width.
- **Typography & color:** system font stack, semantic text colors, dark preference, and contrast
  review are centralized in `design-tokens.css`.
- **Animation:** reduced-motion media query is a global contract; no status relies on motion.
- **Forms & feedback:** labels precede controls; errors use `role=alert`; progress uses polite
  status text and never hides the actionable reason.
- **Navigation patterns:** skip link, `main` landmark, and labelled breadcrumb navigation are
  present; directory navigation remains a button rather than a mouse-only gesture.
- **Charts & data:** existing treemap and tabular summaries remain text-backed; future chart
  changes must provide a table or equivalent text summary in the same story.

## Running the review scenes

```bash
npm run storybook
npm run build-storybook
python3 -m http.server 6006 --directory storybook-static
npm run test-storybook -- --ci --url http://127.0.0.1:6006 --browsers chromium --testTimeout 30000
```

The a11y addon is configured with `a11y.test = "error"`. The interaction stories assert the
materialization-stall cancel event and the disabled checking state. A real VoiceOver/keyboard and
375px/200% zoom pass is still required before a release claim.

## References (APA 7th)

World Wide Web Consortium. (2024, December 12). *Web Content Accessibility Guidelines (WCAG) 2.2*.
https://www.w3.org/TR/2024/REC-WCAG22-20241212/

World Wide Web Consortium. (n.d.). *ARIA Authoring Practices Guide*. Retrieved August 21, 2026,
from https://www.w3.org/WAI/ARIA/apg/

Design Tokens Community Group. (2025, October 28). *Design Tokens Format Module 2025.10*.
https://www.w3.org/community/reports/design-tokens/CG-FINAL-format-20251028/

Storybook. (n.d.). *Accessibility tests*. Retrieved August 21, 2026, from
https://storybook.js.org/docs/writing-tests/accessibility-testing
