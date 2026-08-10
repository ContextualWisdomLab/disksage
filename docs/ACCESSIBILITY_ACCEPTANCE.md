# DiskSage Accessibility Acceptance

## Status

This is a release-evidence contract on the active canonical documentation branch. It does not claim that protected `main`, a particular build, or DiskSage as a whole currently conforms to WCAG 2.2 or ISO/IEC 40500:2025.

The normative accessibility baseline for this document is the published WCAG 2.2 Recommendation and its published ISO adoption, ISO/IEC 40500:2025. A newer ISO/IEC DIS 40500 is under development; it is a watch item only and must not replace the published edition until it becomes a final standard.

## Scope

The acceptance surface is the Tauri/Svelte desktop experience and any HTML-based view shipped by DiskSage. Native operating-system surfaces and third-party provider UI are outside direct implementation control, but DiskSage-owned transitions into or out of those surfaces remain in scope for understandable names, state, error handling, and keyboard continuity.

## Release acceptance matrix

| User need / risk | DiskSage acceptance evidence | Failure rule |
| --- | --- | --- |
| Keyboard operation | Every DiskSage-owned actionable control in the supported buyer journey is reachable, operable, and escapable without pointer input; focus order follows the task flow. | missing, trapped, invisible, or destructive keyboard path blocks release acceptance for that flow |
| Focus visibility and restoration | Focus is visually discernible, dialogs/panels establish an intentional focus target, and closing/cancelling restores focus to a meaningful invoking context. | ambiguous or lost focus is a defect |
| Programmatic names/states | Controls, fields, progress, warnings, approval choices, and destructive boundaries expose meaningful accessible names/roles/states through rendered semantics. | visually implied state without equivalent semantics is a defect |
| Status and error announcements | Long-running evidence collection, success, incomplete evidence, and errors use non-destructive status/alert semantics and do not rely on color alone. | silent or color-only critical state is a defect |
| Contrast and non-color cues | Text, controls, focus indicators, warnings, and charts/indicators meet applicable WCAG 2.2 contrast requirements or provide an equivalent non-color cue. | applicable criterion failure blocks the affected flow |
| Text resize / zoom | Supported desktop zoom or equivalent scaling does not hide critical controls, approval context, or error recovery at the tested release configuration. | clipped/unreachable critical interaction is a defect |
| Target size / pointer alternatives | Small pointer targets have an equivalent keyboard path and applicable WCAG 2.2 target-size requirements are verified for DiskSage-owned controls. | inaccessible primary action is a defect |
| Motion / animation | Non-essential motion respects reduced-motion expectations where motion exists; critical meaning is never conveyed only through animation. | motion-only meaning or avoidable harmful animation is a defect |
| Destructive/sensitive confirmation | Approval, copy/adoption, eviction/reclaim, and other sensitive actions present the operation, target, scope, and cancellation path in perceivable and operable form. | inaccessible consent/confirmation invalidates the flow |
| Screen-reader smoke | Representative release flows are exercised with at least one platform-relevant screen reader before a blanket accessibility claim is made. | absence of evidence means no blanket conformance claim |

## Representative buyer journeys

Accessibility evidence should cover at least:

1. launch -> scan/evidence collection -> result state;
2. Cleanup -> evidence panel -> incomplete/error recovery;
3. cloud/provider candidate review -> approval/refusal -> result;
4. model installation/integrity state where surfaced in UI;
5. settings/help/security disclosure surfaces shipped in the desktop package.

A feature-specific flow may add stricter criteria. Passing one journey does not authorize a universal accessibility claim.

## Automation and manual evidence

Automated semantic/accessibility checks are useful but cannot by themselves prove focus order, screen-reader comprehension, keyboard recovery, platform scaling, or the understandability of a sensitive approval. Release evidence therefore combines deterministic component/browser checks where available with a documented manual keyboard and assistive-technology smoke procedure. Tooling names are deliberately not mandated here until they are integrated and pinned in repository code.

## Evidence identity

Accessibility evidence records the exact source revision, packaged application version, platform, rendering/runtime version when material, tested journey, assistive technology and version for manual smoke tests, result, and known exceptions. A predecessor build, unreviewed screenshot, or active-PR result does not transfer to a release candidate.

## References (APA 7th)

International Organization for Standardization. (2025). *ISO/IEC 40500:2025 Information technology—W3C Web Content Accessibility Guidelines (WCAG) 2.2* (2nd ed.). https://www.iso.org/standard/91029.html

World Wide Web Consortium. (2024, December 12). *Web Content Accessibility Guidelines (WCAG) 2.2*. https://www.w3.org/TR/WCAG22/

### Watch item, non-normative

International Organization for Standardization. (2026). *ISO/IEC DIS 40500 Information technology—W3C Web Content Accessibility Guidelines (WCAG) 2.2* (Draft, Edition 3). https://www.iso.org/standard/94018.html
