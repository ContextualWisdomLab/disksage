# Changelog

All notable changes to DiskSage are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and released versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Unreleased entries describe integrated source changes only; they are not release evidence until the repository's review, CI, security, packaging, provenance, and release-acceptance gates pass on the exact tagged commit.

## [Unreleased]

- Retain temporary Cargo folders when their contents cannot be verified as reproducible; a familiar folder name alone no longer permits a cleanup plan.
- Exclude the reclaim CLI's direct invoking shell from command-line-only activity matches while retaining descriptor evidence and unrelated process matches.
- Keep a completed Trash move successful when its terminal audit record cannot be confirmed, and
  surface a separate next-action warning across cache, development-artifact, and orphan cleanup.
- Revalidate every selected photo group member and block the whole quarantine when a survivor or
  candidate is active, replaced, dataless, or reachable through a hard-link alias.
- Ship the macOS OneDrive Finder post-action verifier as a checksummed, provenance-attested
  operational CLI, and require fresh provider-sync evidence before allocation reduction counts as
  verified local-space recovery.
- Revalidate OneDrive's exact File Provider item/version identity immediately
  before the Foundation eviction call, including every latest sync, policy,
  size, eviction-capability, and Files On-Demand gate, and let approved OneDrive batches use
  the same immutable per-item execution/checkpoint path as iCloud. Finder
  selection remains available only through the explicit `--finder-assistance`
  fallback.
- Keep cloud-inventory argument failures bounded on Unix by reading native
  process arguments explicitly and rejecting non-UTF-8 option payloads with a
  fixed diagnostic instead of allowing Rust's Unicode argument iterator to
  terminate the process before DiskSage can report the next action.
- Keep cloud-local inventory argument errors bounded by rejecting unknown
  options without echoing attacker-controlled option payloads.

- Add an exact-allowlist generated-cache auditor that defaults to dry-run, blocks live processes,
  tool locks, registered or dirty temporary Git workspaces, and provider/Photos/VM boundaries,
  excludes DiskSage and its bounded probes from their own active-use evidence, fingerprints
  cache-internal symbolic links without following them, continues to reject a root symbolic link,
  and requires a fingerprint-bound approval plus a crash-recoverable private JSON Lines receipt before
  removal. Add a plan-first CLI; temporary Git workspaces remain audit-only and route to the
  specialized workspace executor.

### Added

- Add owner-created durable checkout leases for agent or human work that may be idle between turns.
  Active or invalid lease evidence vetoes clone reclamation; expiry is supplied by the owner or the
  lease remains active until exact fingerprint-bound release, without an inferred timeout.
- Reclaim pip downloads, Corepack's Node.js package-manager archive, and each inactive npx
  environment through the existing identity-bound, active-use-checked regenerable-cache action.
- Plan and execute uv's native `cache prune` without `--force`: bind the real executable and cache
  directory, veto active or incomplete `lsof` evidence, require a fresh exact fingerprint, and
  retain immutable approval/result records with filesystem-availability measurements.
- Release verified OneDrive local copies through Foundation only after an exact item-and-version
  fingerprint, current upload/materialization flags, `isKeepDownloaded = 0`, no active handle, and
  attributed approval survive an immediate re-plan. Postchecks and immutable records distinguish
  a successful request from measured allocation recovery; dataless files report zero reclaim.
- Audit non-identical JPEG, PNG, TIFF, and WebP photo candidates with a standards-grounded DCT
  perceptual hash, measured resolution/bit-depth/compression evidence, and an optional unweighted
  Pareto-dominant survivor recommendation. Managed Photos libraries remain excluded; a user must
  select one survivor per group before the other members can move reversibly to OS Trash under an
  exact plan fingerprint and per-item receipt.
- Admit iCloud local-copy eviction from Apple Foundation's public, per-item ubiquitous metadata
  only when the item is uploaded, current, idle, conflict-free, error-free, and not excluded from
  sync. Upload/download errors remain explicit blockers; exact approval is invalidated by the new
  evidence schema, and postchecks now require an uploaded cloud item, `notDownloaded` local state,
  retained ubiquitous path, and reduced allocated bytes.
- Inspect completed DiskSage-owned top-level shared-temporary artifacts through create-only
  lifecycle evidence. Permanent execution and approval fail closed until producer authenticity and
  atomic revalidation, journal, deletion, and receipt durability have an OS-enforced contract; age
  or a same-user marker never grants mutation authority.
- Port the privacy-safe Podman desktop evidence projection into the runtime-orphan stack. The
  customer screen now uses a dedicated read-only IPC schema, keeps every capacity domain optional
  and separate, and no longer renders the detailed internal Podman reclaim plan.
- Verify each registered worktree HEAD against same-repository GitHub PR commit membership so
  squash-merged and detached intermediate commits can be classified without ancestry or branch
  guesses; any exact membership in an open PR takes precedence and preserves the worktree.
- Reclaim regenerable Python tool state from `.mypy_cache`, `.pytest_cache`, `.ruff_cache`, `.tox`,
  and `.nox` through the existing identity, active-use, rescan, and journal safety contract;
  `setup.cfg` discovery recognizes the exact tox `[tox:tox]` section.
- Reclaim downloaded Playwright browser runtimes through the same regenerable-cache contract.
- Exclude images retained by Podman/Buildah external storage containers from orphan deletion plans.
- Repair inconsistent Podman storage through an explicitly machine-scoped, non-forced native
  command and retain attempted-operation receipts plus bounded postcheck evidence after failures.
- Reclaim project-local Python 3.14 `.venv314` environments as regenerable development artifacts.
  Every discovery path verifies bounded `pyvenv.cfg` metadata for Python 3.14, skips rejected
  environment trees without recursively scanning them, and the
  cleanup screen names each Python cache and test environment so the next action is clear.
- Probe Apple's public File Provider identity boundary for uploaded, current, idle OneDrive files
  without changing materialization. The real provider returned `ProviderNotFound`, so execution
  remains blocked while retaining the existing approval and receipt contract. Provider-wide
  new-copy admission remains confined to copy/upload workflows and cannot deadlock local-space
  recovery while unrelated downloads, indexing, or historical provider errors exist. If the
  native completion stalls, DiskSage terminates only its private helper process group. The macOS 11
  helper binds item and domain identity and rejects replacement paths; vendor-private `/unpin`,
  OAuth, and cloud-object deletion are not used.
- Select only freshly replanned, fingerprint-approved OneDrive items in Finder through public
  AppKit, then direct the customer to choose **Free Up Space** and verify retained provider identity
  plus reduced allocated bytes. Finder selection is recorded as non-mutating assistance and never
  reported as reclaimed capacity; private action invocation and Accessibility automation remain
  excluded.
- Partition iCloud eviction manifests automatically: keep freshly verified, fully uploaded local
  copies in the approval batch and exclude sync-incomplete items without exposing their paths.
- Extend the same batch planner, exact fingerprint approval, live re-plan, immutable checkpoint,
  and post-allocation verification contract to OneDrive Files On-Demand. The generic
  `disksage-cloud-local-eviction-batch` CLI replaces the provider-specific batch command name.
- Ship that generic batch planner as a checksummed, provenance-attested macOS operational CLI so
  installed release artifacts can reproduce the exact read-only plan. Linux and Windows remain
  excluded until their provider-local observation paths have production evidence.
- Ship the iCloud-named batch planner as a separate checksummed macOS artifact so operators can
  generate the native Foundation-backed plan without compiling source; Linux and Windows retain
  only the provider-generic planner because the iCloud Foundation contract is macOS-specific.
- Ship the read-only cloud-local allocation inventory as a checksummed macOS artifact so a
  matching release can produce fresh candidate evidence before either batch planner runs. An
  inventory from an older executable head remains stale and cannot authorize a new plan.

### Fixed

- Preserve a failed Podman native storage-repair attempt as an explicit provider refusal when a
  damaged layer remains container-referenced; DiskSage now directs a new lineage/removal evidence
  cycle instead of retrying, forcing repair, or touching graph-driver storage.

- Make the release-packaged cloud-local inventory producer return help on stdout with exit status
  zero, while mixed help/runtime arguments remain a bounded failure.
- Retry transient iCloud metadata failures during the bounded post-eviction check instead of
  misreporting temporary evidence unavailability as lost cloud identity.
- Match canonically equivalent macOS Unicode spellings when proving that a selected File Provider
  item is beneath its discovered cloud root; sibling roots remain rejected component by component.
- Add an explicit `--execute --permanent` development-artifact mode that physically removes only
  a freshly rescanned, inactive, identity-matched generated directory and journals the irreversible
  outcome; the default remains reversible OS Trash.
- Reclaim Superset's isolated HTTP and compiled-code caches while retaining cookies, local and
  session storage, IndexedDB, preferences, and historical network diagnostics.
- Reclaim only VS Code, VS Code Insiders/Server, and Cursor extension directories named by each
  editor's native `.obsolete` lifecycle metadata, with bounded manifests, symlink rejection,
  identity revalidation, Trash, and journaling.

- Catalog AppMap downloaded tool binaries as regenerable macOS data. Superset network diagnostics
  remain separately visible for explicit review because historical logs cannot be regenerated.
- Add standalone stale-PR clone reclamation: only a clean, inactive, single-worktree clone whose
  exact branch and head OID match fresh same-repository GitHub evidence can move to OS Trash.
  Branch deletion, Git pruning, detached clones, dirty clones, and implicit age thresholds remain
  prohibited. The same contract is available through a headless plan-first CLI with exact human
  confirmation and an external append-only journal.
- Add bounded multi-root standalone-clone inventory and fresh default-branch ancestry authority.
  Provider OID and local remote-tracking reference must match exactly; stale, dirty, active,
  unpublished, and diverged clones remain fail-closed without any age heuristic.
- Add an explicit operator-supplied cutoff for stale same-repository open pull-request worktrees
  (ADR-0015). GitHub creation time, state, branch, and exact head OID are refreshed before each
  removal; branches and commits remain untouched and no implicit age threshold is used.
- Expose current same-repository closed-PR and explicitly stale-open PR evidence through the
  headless worktree audit/removal CLIs, with the same live re-audit and exact approval contract as
  the desktop application.
- Reclaim clean, inactive worktrees for same-repository pull requests closed without merge only
  when GitHub reports an exact branch-and-head match; refresh that evidence before each removal
  and preserve fork, detached, dirty, active, or changed worktrees.
- Reclaim clean, inactive worktrees whose exact branch and head match a same-repository merged pull
  request even when squash or rebase history does not retain that head. Closed-unmerged and merged
  evidence use separate bounded GitHub queries, and merged lookup is scoped to branches currently
  registered as worktrees so repositories with long merged histories remain auditable. All lookup
  calls consume one shared timeout budget rather than multiplying the configured wait per branch.
- Exclude macOS Photos library packages from exact-duplicate traversal and reject a managed Photos
  library selected as the scan root. External files remain auditable without interpreting Photos'
  private databases and derivatives as independent duplicate-delete candidates. Reclaim also
  canonicalizes every approved member immediately before mutation and fails closed if a replaced
  parent symlink redirects it outside the audited root or into a managed Photos library.
- Stage each verified duplicate by filesystem identity before permanent removal, restoring rather
  than deleting a pathname replacement that races the approved audit; receipts distinguish active
  skips from failed removals and retain stable failure reasons.
- Apply the single GitHub evidence deadline to desktop worktree planning, desktop removal, the
  removal CLI, and every mutation-boundary live re-audit instead of refreshing the timeout for
  each pull-request lookup.
- Add runtime-agnostic container orphan reclamation (ADR-0012): one fail-closed engine audits
  stopped containers, unreferenced images, dangling volumes, and unused custom networks across
  Docker (native), Colima (`docker --context colima`), and Podman machines. Every execution
  re-audits immediately before mutating and requires an approval phrase embedding the SHA-256
  fingerprint of the exact candidate identity set; running or paused containers, tagged images,
  built-in networks, and attached volumes are never candidates. Exposed via the Cleanup screen
  with confirmation gating, bounded rationale input, and actionable failure copy, plus a
  read-only `disksage-container-orphan-plan` CLI for headless evidence.
- Report Docker dangling-image reclaim bytes from the runtime's numeric `image inspect` size, never
  by converting the human-readable listing with a unit heuristic; missing or mismatched identity
  evidence keeps the category blocked.
- Pin Docker-native approval and execution to the same resolved daemon endpoint so mutable context
  configuration cannot redirect an approved deletion.
- Preserve an indeterminate mutation receipt after a started exact-delete command exits non-zero,
  times out, or loses capture evidence; the UI directs customers to refresh instead of reporting
  the partially applied operation as untouched.
- Include shared temporary storage (`/tmp`, or macOS `/private/tmp`) in the cleanup catalog. Only
  current-user-owned, non-linked trees with a complete ownership walk can become identity-bound
  Trash targets; the shared root and other-user/system-owned objects remain protected.
- Add Podman/Colima VM storage maintenance planning (ADR-0014): inspect guest state and offer a
  bounded, exact-phrase-approved `fstrim` operation. Host VM-image compaction remains explicitly
  unsupported until a runtime-native integrity proof exists; no VM image, volume, or user file is
  rewritten by this feature.
- Run bounded runtime trim and recovery waits on Tauri's blocking pool so long guest maintenance
  cannot occupy asynchronous command workers.
- Preserve Docker context TLS credentials by executing through the explicitly pinned context while
  binding approval to the complete inspected context definition.
- Surface an approved duplicate at a deterministic sibling recovery name when its original path is
  concurrently occupied, and report preservation or rollback failure explicitly.
- Detect a running but unreachable Podman/Colima guest, offer a separate exact-phrase-approved
  runtime-native stop/start recovery, and re-check reachability before enabling trim. Trim receipts
  now include bounded before/after host-volume evidence for the measured available-space change.

### Changed

- Build cache cleanup fingerprints from complete bounded filesystem metadata instead of reading up
  to 4 GiB of generated file content. Large and sparse cache entries remain reviewable, while exact
  identity and metadata are revalidated immediately before any mutation.
- Revalidate reviewed cache manifests on both sides of atomic Trash staging and restore the
  original path when evidence changes instead of moving unreviewed contents.
- Preserve the original approved pathname during final staged active-use probes so command-only
  users block permanent cache and development-artifact deletion.
- Stop descending once a marker-validated development artifact is found, avoiding a second full
  traversal of large nested `node_modules`, `target`, and generated index trees before cleanup.
- Keep a partially failed permanent artifact deletion in its private staging location; never restore
  a partially removed tree to the live path as if it were intact.
- Require complete inactive-use evidence for every development artifact immediately before Trash,
  including `node_modules`, Rust targets, generated indexes, and editor-obsolete extensions.
- Resolve macOS cache roots from the effective XDG/UV environment and observed native locations:
  `~/.cache` for uv, Codex runtimes, Node, PyTorch, Prisma, and GitHub CLI, plus
  `~/Library/pnpm/store` for pnpm's content-addressed store. The existing guarded cleanup keeps
  identity, active-use, Trash, and journal gates; caches without an established automatic policy
  remain manual-review candidates.
- Recognize macOS Trash collision-renamed cache directories only when their known base name and
  cache-specific directory structure both revalidate, including uv git and pnpm registry metadata
  caches; arbitrary Trash entries remain excluded from permanent purge.
- Extend the same structural purge proof to uv archive caches and pnpm v10/v11 store layouts while
  preserving the bounded no-symlink traversal and pending/terminal journal records.
- Reject control characters in the Podman/Colima VM-trim rationale before any runtime probe or
  receipt write, keeping maintenance records bounded and consistent with other actions.
- Fix Colima runtime-state parsing to retain validated status values before temporary JSON data is
  dropped; Rust hosted test compilation now remains borrow-safe while invalid state still fails
  closed.
- Clarify reclaim-domain contracts and customer actions: exact-content photo groups remain
  reversible, non-identical photos require a manual comparison, and cleanup messages no longer
  expose implementation details.
- Verify Podman network membership through its container listing, follow installed CLI symlinks,
  hide unavailable runtime panels behind one actionable summary, terminate runtime subprocess
  groups on timeout, and exclude merged history before bounding closed-PR evidence.
- Keep coverage builds compile-safe by applying the same `not(coverage)` boundary to native-copy
  identity cleanup and dependent eviction helpers; the focused authority contract remains green.
- Add durable private failure records in a separate journal directory and a receipt-bound
  cancellation command for bounded native cloud copies; bind cancellation to the active candidate,
  require provider-native local-current materialization evidence before existing-copy adoption can
  hash a destination, cap the private failure journal at 10,000 records, and bind failed-copy
  cleanup to Unix/Windows file identity while keeping shareable lineage exports path-free. Existing
  copy adoption remains explicitly non-cancellable because it performs verification only.
- Persist bounded, path-free local-volume snapshots from cloud plans with create-only files,
  content fingerprints, Unix `0400`/`0700` permissions, and shape-limited retention; surface a
  warning when incident-comparison evidence cannot be written without changing copy authority.
- Persist path-free provider-client process observations with the same bounded, create-only
  evidence contract so a stalled File Provider incident can be compared across planning loops.
- Persist redacted iCloud queue and File Provider activity summaries as bounded, create-only,
  timestamped evidence records, without retaining raw CloudDocs databases or provider dumps;
  surface persistence failure without changing copy or eviction authority.
- Gate iCloud copy plans on a path-free three-stream evidence cohort with deterministic
  fingerprints and a five-minute observation-skew ceiling; incomplete, malformed, or stale
  observations remain blocked and never become cloud-write or eviction authority.
- Carry the integrity-checked iCloud pre-copy cohort and `pre_copy_evidence_met` through the
  Naruon cloud-copy readiness envelope (schema version 8), so aggregate consumers also fail closed
  when the provider queue is quiet but pre-copy evidence is absent.
- Keep the hourly contextual-orchestrator loop on its published read-only API, bind context to
  the exact event commit, and remove foreign-repository checkout, KV mutation, and provider-secret
  ingestion from DiskSage Actions.
- Keep the repository-local contextual-orchestrator advisory workflow manual-only and bind the
  hourly OpenCode review/repair schedule to the trusted central `.github` scheduler, avoiding an
  unpinned autonomous model reviewer while retaining exact-head, read-only evidence boundaries.
- Show the last read-only iCloud File Provider evidence timestamp beside the
  new-copy admission state, so a stalled `no progress`/`hard expired` queue has
  an actionable retry context without exposing provider paths.
- Bind Tauri packaging to a fail-closed cross-manifest release-version verifier so `package.json`, `Cargo.toml`, `tauri.conf.json`, and any `v*` release tag must agree on one valid Semantic Version before a bundle is built.
- Add retry-safe release concurrency: fresh first attempts may supersede stale runs, while explicit GitHub rerun attempts do not self-cancel inside the same concurrency group.
- Replace generator-era Cargo package metadata with the DiskSage product description, MIT license expression, canonical source repository URL, and `publish = false` registry-publication boundary; deliberately omit Cargo's deprecated `authors` field, verify publication refusal through Cargo's versioned parsed metadata rather than substring matching, and regression-test commented/out-of-table decoys together with the retained acquisition metadata and doctoring evidence.
- Require a fresh, exact, human-attributed approval and rationale for cloud copy-only and existing-copy adoption actions, with a 15-minute authorization lifetime bound to the candidate, destination, provider, account scope, and review fingerprint.
- Return the candidate-specific cloud copy approval action, exact confirmation phrase, and maximum approval age from the Rust plan contract; the frontend only displays and submits that backend-authored phrase and fails closed when it is missing or does not match the candidate action.
- Align the frontend toolchain on Vite 8.2 and `@sveltejs/vite-plugin-svelte` 7.2 so the declared peer dependency graph is installable and reproducible.
- Declare the supported Node.js runtime floor as Node.js 20.19 or Node.js 22.12 and later, matching Vite 8 requirements.
- Pin the primary test workflow to Node.js 20.19.0 so the minimum supported runtime is continuously verified.
- Document the iCloud batch operation's local-only versus path-free shareable evidence boundary and map its fail-closed controls to NIST SP 800-53 Release 5.2.0, ISO/IEC 27040:2024, and primary secure-design literature with APA 7th references and deterministic documentation contract tests.
- Refresh the Tauri CSP standards evidence to the current July 29, 2026 W3C Content Security Policy Level 3 Working Draft and regression-test its exact publication URL so future doctoring cannot silently drift back to an older draft.

### Fixed

- Use macOS `NSFileManager` for reversible Trash moves so cleanup does not wait on Finder
  AppleEvents or inherit a stalled Finder copy queue.
- Permit fully current-user-owned real children of the shared Unix temporary root while retaining
  fail-closed protection for the root, symlinks, mixed ownership, unreadable trees, and oversized
  ownership observations.
- Reject ontology organize destinations that are relative to the process working directory,
  named-user tilde paths, or parent-traversal paths; only an absolute destination or a home token
  (`~`/`~/`, plus native Windows `~\`) can produce a move plan, and literal tildes in absolute
  paths are preserved.
- Surface the bounded iCloud File Provider upload/download fractions and label repeated
  `no progress` observations as a Finder “copy preparing” stall, so the operator can cancel the
  pending Finder request before retrying; this remains diagnostic and never grants copy or eviction
  authority.

- Keep the shipped Naruon readiness verifier source includable by its integration boundary test;
  the terminal parser contract now compiles in both the binary and test-module contexts.

- Cover the `sensitive-config` archive-kind wire label in the generated cloud-plan implementation,
  so the macOS/Linux/Windows cloud-plan binaries compile after the sensitive-config safety
  boundary is enabled.
- Keep the local staging-headroom gate on new native copies only; existing-copy adoption now
  remains available on low-disk volumes because it verifies an already-present destination without
  creating local staging data.
- Surface insufficient local staging headroom as a dry-run notice and native-copy blocker before
  review, while keeping the non-staging provider-API fallback and existing-copy adoption available.
- Make `disksage-duplicate-audit --help` exit successfully so release staging can
  verify its usage contract without treating a help request as a failed audit.
- Publish the source-bound SPDX SBOM as a separately named artifact only after
  provenance succeeds, then download it in the release publication job so the
  attested 18-file release set cannot silently omit its component inventory.
- Isolate the macOS global File Provider dump helper in a private process group and terminate the
  whole group on timeout, preventing descendant helpers from retaining a pipe after a stalled
  Finder/provider copy.
- Classify repeated File Provider `-1005 itemNotFound` markers as a path-free global sync blocker,
  retain the same-blocker duration when reconciliation counts change, and direct operators to
  cancel a stalled Finder copy before retrying.
- Reject legacy provider evidence that reports `sync_complete=true` without an explicit complete
  `sync_state` at the current authorization and eviction boundary, while retaining compatibility
  reads and a public-boundary regression test.
- Replace the unmaintained direct `jwalk` production dependency with the maintained `walkdir`
  backend across scanner, duplicate, artifact, cloud, and reclaim traversals; preserve symlink/
  reparse filtering and fail-closed traversal-error accounting with a locked dependency contract.
- Inventory direct credential-bearing configuration names as blocked `sensitive-config` entries;
  never open them for metadata probing or include them in cloud-copy or source-eviction authority.
- Bound one-minute background reconciliation to 128 immutable provider evidence records per
  receipt, and validate active iCloud File Provider transfers as blocked readiness evidence.
- Scope persisted API object-id recovery by receipt filename prefix before scanning, so unrelated
  receipts in a shared evidence directory cannot hide a valid Google Drive or OneDrive locator.
- Add an explicit macOS Finder-copy cancellation command that sends only a bounded Escape request;
  it never accepts scripts or paths and never terminates iCloud/File Provider services.
- Fail closed when OneDrive or Google Drive runtime evidence is unavailable during client recovery;
  an unknown observation is no longer treated as proof that the provider process is absent.
- Hardened iCloud local-copy batch eviction with fresh per-item timestamps, deterministic planner/executor/recorder/clock seams, fail-closed immutable checkpoint handling, bounded manifest admission, symlink-safe control-path validation, and distinct operator diagnostics.
- Restored the cloud-copy public documentation regression contract after a temporary repair path removed it, so CI continues to fail when the new Rust or TypeScript approval surfaces lose beginner-readable documentation.
- Align release artifact verification with the pinned `windows-2022` build matrix name, and make the container-capacity regression fixture satisfy the same runtime-health probe required in production.
- Require standalone-clone cleanup to bind a real in-root Git directory, complete audit evidence, and an external safe journal before an approved Trash move.

### Security

- Default personal cloud-provider OAuth consent to read-only; upload scope and API write
  authority now require an explicit user opt-in.
- Catalog the Cargo registry source tree as an explicit, identity-bound regenerable-cache target;
  keep it out of automatic cleanup because rebuilding may require network downloads.
- Catalog the observed PyTorch, Prisma, and GitHub CLI cache trees as identity-bound manual-review
  targets; keep them out of automatic cleanup until their active-use and rebuild contracts are
  independently established.
- Add buyer-verifiable release artifact provenance with read-only platform build jobs, a tag-only least-privilege attestation job, exact 18-file admission including a source-bound SPDX SBOM, adjacent operational-CLI SHA-256 verification, preserved artifact namespaces, non-regular-entry rejection, and a separate publication job that cannot publish before attestation succeeds.
- Require explicit organization-tenant authority when either the destination account scope is organization-owned or the canonical organization-sensitive review reason is present; fail closed in both frontend projection and durable Rust transfer authorization even when the ordinary review flag is absent, and regression-test contradictory signal combinations.
- Enable an explicit fail-closed Tauri Content Security Policy to keep executable scripts and fonts local, grant production network authority only to the Tauri IPC transport, confine Vite WebSocket HMR to a separate development-only CSP, deny object/frame/base-URI authority, deny form submissions with explicit `form-action 'none'`, deny unused worker, media, and web-app-manifest fetch authority with explicit `'none'` directives, and regression-test against null, wildcard, remote-script/style, eval, and development-authority leakage.
- Re-verify the installed GGUF immediately before llama.cpp initialization and retain the verified model handle through llama.cpp loading: reject missing, linked, non-regular, identity-raced, short, oversized, unreadable, or SHA-256-mismatched artifacts with stable path-free errors; use a stable descriptor path on Unix and a Windows read-sharing guard so the mutable source pathname cannot be substituted between verification and model parsing.
- Bind the default on-device GGUF model to an immutable upstream revision, exact byte count, and SHA-256 digest; replace whole-model buffering and named sibling staging with bounded streaming into an unnamed same-directory temporary file; ignore and preserve unrelated legacy `.part` paths; refuse destination overwrite with create-new semantics; capture destination ownership from the returned open file handle; re-read and rehash the still-open staging source while copying; flush, sync, re-read, and rehash the destination before final acceptance; reject same-file source or destination mutation; preserve foreign destination replacements through identity-bound cleanup; and keep model installation inside the Rust coverage surface with privacy-safe stable errors and deterministic race regressions.
- Persist copy-approval provenance in immutable receipt lineage, reject stale, generic, mismatched, or tampered approvals, and retain explicit backward readability for pre-approval receipt formats.
- Generate the npm lockfile in an exact-head validation job with repository contents read-only and dependency lifecycle scripts disabled, bind the artifact to SHA-256 evidence, and grant `contents: write` only to a separate publication job that verifies the same-run artifact and unchanged branch head before committing the lockfile.
- Removed obsolete one-shot repair workflows and patch scripts so repository automation no longer retains dormant write-capable recovery paths.
