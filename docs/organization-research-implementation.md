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
