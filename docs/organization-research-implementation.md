# Evidence-based organization implementation

Status: in progress. Research observations are not shipped capability evidence.

The organization planner and execution source validator now preserve package descendants and companion-file relationships. Same-basename files with different extensions are retained, not treated as duplicate content. Execution inspects current siblings so a companion omitted from a bounded inventory cannot be silently separated. Incomplete sibling inspection refuses the individual move.

## Remaining product work

- Validate retained-item rendering and empty preview behavior in the actual app. The response and Organize now include package, companion and unplanned-item reasons.
- Represent content-grounded project/activity bundles, document roles, concept facets and unresolved evidence without inventing topic confidence.
- Preserve all distinct drafts; identify final versions only from authoritative evidence.
- Keep low-context transcripts unclassified and preserve source-relative audio/transcript/metadata relationships.
- Implement native coordinated iCloud bundle moves with exclusive destination creation, content-bound manifests, fresh shared/upload/conflict checks, durable receipts and verified undo. The operational Swift helper is not yet part of the application.
- Keep local move verification distinct from provider upload completion and remote-device verification; moving files contributes zero reclaimed bytes.
- Exercise the public planning, execution and undo paths against realistic synthetic fixtures; do not commit private user documents as fixtures.

The first boundary guard is a safety prerequisite, not completion of semantic organization or the 300GiB goal. Existing filename/extension classification is not a content-based ontology implementation.

## Validation checkpoint

- Standalone production boundary tests: 2 passed, including a symlink alias into an application package.
- Existing frontend API wrapper tests: 10 passed. These verify command forwarding, not native move safety.
- Svelte diagnostics: 0 errors and 0 warnings; the latest Organize component also compiled without warnings.
- Rust organization tests: 25 passed, including metadata binding, companion preservation, package exclusions and retained-item reporting. The latest command-path suite passed 29 tests, including preservation of an unlisted companion, no move journal on refusal, ordinary moves and undo collision handling.
- Repository-wide format checking reports pre-existing differences outside this change; no mass formatting was applied.

The current sibling observation is a pre-execution check, not a filesystem transaction: an uncoordinated writer can still add a companion after it. Native coordinated bundle execution and late-writer handling remain required. The preview covers a bounded inventory and explicitly does not attest whole-tree completeness. Shared/session/project-marker boundaries beyond recognized package suffixes remain part of the pending implementation, not inferred coverage from these tests.

## Native integration findings

The product already depends on objc2 0.6.4 and objc2-foundation 0.3.2 and calls Foundation directly for iCloud state/eviction. Reuse that boundary for moves instead of invoking a Swift interpreter on the user's machine. The installed Foundation bindings expose NSFileCoordinator's two-writing-URL accessor, ForMoving option, and willMoveTo/didMoveTo notifications. Their coordinator, presenter and block features are not currently enabled in this product.

The accessor must use the URLs supplied by Foundation, revalidate the approved content manifest inside coordination, and perform an exclusive rename. Coordination is not evidence that an uncoordinated writer cannot modify contents. Persist a pending receipt before mutation and retain an inspection-required result on ambiguous completion; do not automatically replay a move. Existing single-file hard-link execution does not satisfy this bundle contract.

Context7 documentation lookup returned a quota error. These findings come from the installed version's generated bindings and existing product source; they do not constitute a native runtime test.

Destination follow-up: package destinations are excluded during planning; execution also resolves the nearest existing destination ancestor to reject aliases into packages. The updated organization suite passed 28 tests, including the destination-plan regression and native symlink fixture. This remains a pre-execution check, not an atomic filesystem guarantee.

Probe-budget regression: a 201-item fixture with a 200-probe budget reproduced 201 executable plans (RED, expected 200). The exhausted-budget branch now withholds a plan instead of substituting empty metadata. The omitted item remains visible in the retained preview. Post-fix organization tests passed 28/28; this change does not claim semantic classification for the first 200 items.

## Session preservation during preview

The planner now reuses the shared agent-state guard before classification and rejects destinations in protected state. The preview explains retained session files. Execution continues to use the same protected move path supplied by PR #345; PR #346 is stacked on that branch until its protected merge. A regression case checks both source preservation before the picker and protected destination rejection.

Validation after shared-guard integration: 29 organization tests passed with `cargo test --manifest-path src-tauri/Cargo.toml --lib organize:: --no-default-features --offline`; `npm run check` reported zero errors and warnings. The metadata-budget fixture took over 60 seconds after path resolution was added; this is a latency observation requiring investigation, not a failed test or a throughput guarantee.

## Exact undo paths

A Unix filename containing ` -> ` reproduced a failed undo in the public command core. New move receipts now contain separate source and destination fields; the display string is no longer parsed for new receipts. Legacy receipts remain readable, but ambiguous legacy path strings are skipped instead of guessed. The focused command regression failed before this fix; all 29 command tests passed after the fix. This change alone does not provide coordinated iCloud transactions, crash-durable receipts, or protection against replacement of a moved file before undo.

The exact shared guard measured 458 ms for the synthetic `/home/u/Media/Image/0.png` path, versus less than 1 ms for `/downloads/0.png` and a `/tmp` path in the same process. The metadata-budget fixture now supplies an actual temporary home directory. Production protection remains unchanged; this observation does not establish production throughput.

Follow-up validation: all 47 shared safety tests passed for structured undo receipts. The metadata-budget regression passed in 0.06 seconds with the actual temporary home, compared with the previous 91.41-second organization run. Same-volume movement now calls the existing exclusive rename primitive instead of linking then unlinking; all 47 shared safety and 29 command regressions passed after this change. Native iCloud coordination remains unfinished.

## Native coordinated move integration

Same-volume macOS movement now uses Foundation file coordination before the exclusive rename. The callback rejects changed URLs, rechecks agent-state protection and the source and resolved destination-parent identities, then announces the successful native move before writing its final journal outcome. Forward moves and undo share this path. Cross-volume movement is unchanged.

This uses the installed objc2-foundation 0.3.2 bindings and block2 0.6.2 already present in the lockfile; no runtime Swift compiler or helper process is needed. Context7 was unavailable because its monthly quota was exhausted, so the exact installed bindings and [Apple's coordination reference](https://developer.apple.com/documentation/foundation/nsfilecoordinator) and [move notification reference](https://developer.apple.com/documentation/foundation/nsfilecoordinator/item(at:willmoveto:)) were inspected.

The native safety suite passed 47 tests, and the command suite passed 29 tests including forward movement and undo. Foundation emitted sandbox-extension diagnostic messages in the test process even though the operations and preservation assertions passed; this is not evidence of signed-app sandbox entitlements. Local coordinated movement does not prove iCloud upload completion, cross-device consistency, crash-durable recovery, or exclusion of writers that do not participate in file coordination. General recursive bundle manifests and crash-durable receipts remain unfinished.

The organizing command now supplies its existing plan validator to the shared move transaction. It runs before preparation and again inside the native accessor (or immediately before non-macOS/cross-volume mutation). A synthetic companion arriving after preflight first reproduced a missing revalidation, then passed with source and companion contents intact and no destination file. All 30 command and 47 safety tests passed after the fix. This does not exclude uncoordinated writers racing after the final validation.

## Whole-folder capability audit

The `organization_lineage` export carries path-free per-file metadata and cannot authorize a folder move. The initial audit found no content-bound membership manifest in `MovePlan`; the bounded pilot below now adds one. Existing orphan and developer-artifact manifests remain metadata-only cleanup guards and are not used as proof of document content preservation. Semantic grouping is still incomplete.

The bounded pilot binds complete membership, raw relative names, file contents, and modification metadata to its supported bundles and revalidates this for movement and undo. General recursive bundles remain unsupported. It must preserve packages, project and sharing boundaries and retain incomplete or unavailable cloud content. Content/ontology grouping needs a separate evidence-backed decision: equal extensions, shared basenames, or an existing parent alone cannot establish semantic equivalence. Existing companion preservation remains a veto, not a grouping verdict. Folder movement must not be counted as reclaimed storage.

## Existing-folder movement pilot

The UI now previews an explicitly selected existing folder and destination parent. It does not infer a topic or ontology class. The current supported scope is a flat bundle of at most 32 local regular files totaling 512 KiB. Nested, unavailable, linked, cloud-only, recognized project, and protected package/session scopes are retained. General recursive and semantic grouping remain unfinished.

The move plan and undo receipt now bind member names, file identities, sizes, exact modification timestamps, and content digests. The same plan validator runs inside native coordination. Movement across volumes is unavailable for these bundles. Tests have verified a decomposed-Hangul folder name, complete companion movement and undo, refusal after a new member appears, and same-size/same-mtime content drift. The first completed command run passed 31 tests and failed the unchanged live cache-cleanup assertion. After adding error diagnostics and the project-boundary regression, all 33 command tests passed. The intermittent cache assertion was not reproduced in that run; its underlying cause is not established by this result. All 47 shared safety tests also passed. The latest frontend check reported zero errors and warnings. No real user folder was moved by this pilot validation.
