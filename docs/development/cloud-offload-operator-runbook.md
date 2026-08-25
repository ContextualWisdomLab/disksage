# Cloud offload runtime evidence

DiskSage plans cloud copies from embedded metadata first, then filename date, filesystem creation
time, and modification time. Filename tokens such as `2026-04-28` or `251210` are secondary
evidence and never establish production time by themselves.

Ontology-based local organization uses the same precedence. Its preview performs a complete
bounded scan (10,000 entries or 10 seconds), records a path-free lineage fingerprint and source
size/mtime snapshot; execution rechecks both immediately before a move and skips File Provider
`dataless` sources. When the local model is enabled, its class prompt receives the same bounded
production-time evidence; metadata probing is capped at 32 files per planning pass. A partial scan
is rejected rather than producing a partial move plan.

The runtime sequence is:

1. A verified copy writes `cloud-goals/<receipt-id>-latest.json` with provider and evidence gates
   explicitly incomplete. The desktop app uses its app-data directory; the headless CLI derives
   sibling `cloud-goals` and `cloud-adr` directories from its receipt/evidence directory.
2. Provider attestation writes an immutable evidence record, then updates the Goal and ADR
   projections atomically in both entrypoints.
3. After restart, the desktop app automatically runs `reconcile_cloud_receipts` over the bounded
   receipt set; it refreshes provider evidence and the replaceable ADR/Goal projections locally.
   The open desktop view repeats this cloud-write-free reconciliation every 60 seconds and exposes
   a manual re-run; it never evicts a source.
   The same operation is available headlessly with
   `disksage-cloud-plan --reconcile-receipts --receipt-dir ABSOLUTE_PATH --evidence-dir ABSOLUTE_PATH`
   (add the existing OAuth connection flags only when a provider API fallback is required). This
   command performs local evidence/projection writes only; `--audit-receipts` remains a strictly
   read-only integrity report.
   When the local File Provider cannot admit a new OneDrive or Google Drive item, an explicitly
   approved headless upload can use `--provider-api-copy-fingerprint HEX64` together with
   `--oauth-connections ABSOLUTE_PATH`. This path requires the write OAuth scope, fresh capacity,
   the same human-attributed copy phrase and review gates, re-hashes the source after upload, and
   immediately attempts provider-API attestation. It never supports iCloud or local eviction in
   the same action; the returned receipt/object ID is the hand-off for a later attestation.
   For iCloud plans, `pre_copy_evidence` also exposes a path-free cohort of
   volume-pressure, provider-runtime, and iCloud-health observations. Missing,
   incomplete, malformed, or more-than-five-minute-skewed observations produce
   stable blockers and prevent a new copy. This cohort is a freshness check;
   it is not per-item upload attestation or an eviction permit.
   Native File Provider copy buttons are admitted only when the current source volume has the
   candidate size plus a 1 GiB staging reserve available. The same check is repeated immediately
   before the copy; `local-volume-headroom-insufficient` is fail-closed and must not be worked
   around with Finder folder copies. Provider-API upload is the explicit non-staging fallback.
   On macOS, DiskSage's own native copy prepares the destination with bounded `/bin/mkdir -p` and
   runs `/bin/cp` in private child processes rather than using a Finder folder operation. The
   timeout is derived from the candidate size and capped at 30 minutes; timeout/helper failure
   removes only the partial destination, returns `cloud-copy-timeout` or
   `cloud-copy-helper-failed`, and leaves the source unmodified for a fresh plan.
4. `is_local_current=true` with `is_uploaded=false` is `pending-upload`; the source remains and
   no eviction permit is issued.
   iCloud native `needs-sync-up` and `needs-sync-down` states are also explicit admission blockers;
   a timeout while collecting native status is an admission blocker as well;
   the bounded iCloud File Provider activity probe likewise blocks when it sees redacted
   `no progress` fetches, active upload/download progress, or times out;
   the latter means the provider still has remote changes to materialize.
   While this evidence is blocked or unavailable, automatic probes back off to five-minute
   intervals so DiskSage does not add repeated readers to an already busy File Provider database;
   copy admission remains fail-closed during the backoff. The UI's explicit `iCloud 상태 즉시
   재확인` action may perform one bounded read-only probe after the operator cancels a stuck Finder
   copy; it does not restart `bird`, write cloud data, or evict a source.
   If CloudDocs `client.db` exceeds the bounded snapshot ceiling, DiskSage skips the expensive
   SQLite fallback and reports incomplete evidence instead of waiting indefinitely; the File Provider
   probe remains bounded and still blocks new copies.
   Third-party File Provider dumps also block new copies while upload/download progress,
   non-zero reconciliation backlogs (`provider-global-sync-reconciliation-pending`), provider
   disconnection, local disk-full (`provider-global-sync-local-disk-full`), or path errors are present; the stable blocker codes are shown in the plan and
   are never a reason to bypass the gate.
   The UI's `공급자 앱 재기동 후 상태 재확인` action is available for those OneDrive/Google Drive
   blockers. It targets only the verified desktop-app bundle, requests a bounded quit and relaunch,
   and records whether the process is observed afterward. A missing post-restart observation still
   blocks copying; the action performs no cloud write, attestation, or source eviction. iCloud's
   system-managed `bird` process is intentionally not terminated by DiskSage.
   When a third-party provider reports active transfer, reconciliation backlog, disconnection, or
   server/error blockers, the same panel exposes `Finder 복사 취소 요청`; it sends only the fixed
   bounded Finder Escape action and immediately rechecks the provider-global dump. This is not a
   provider restart or copy/eviction authority.
   The identical bounded action is available headlessly:
   `cargo run --features cloud-cli --bin disksage-provider-recovery -- --provider google-drive`
   (or `onedrive`). A non-zero result such as `provider-recovery-quit-request-failed` is evidence
   that the provider remains blocked; do not fall back to Finder folder copies or force-kill the
   provider. If the app is unresponsive to AppleScript, the explicitly approved escalation is
   `--allow-graceful-term`; it sends SIGTERM only to the fixed verified app name and still requires
   a fresh post-restart provider dump. Use `--output ABSOLUTE_NEW_FILE.json` when a 0600 recovery
   receipt is needed.
5. If the destination is valid but the receipt source is missing, unsafe, or macOS reports it as a
   File Provider `dataless` object, reconciliation writes a blocked Goal/ADR projection and records
   `source-not-present`, `source-content-not-local`, or the precise source-state blocker; it never
   treats that as proof of a completed eviction. A previously advanced projection is not rewound;
   its Goal status becomes `blocked` and the explicit eviction gate is revoked. A terminal
   `source-evicted` projection remains completed because the original path is expected to be gone.
   Files under macOS File Provider's private `File Provider Storage` tree (including
   `DownloadStage`) are also non-overridable `system-managed-file-provider-storage` blockers;
   DiskSage never treats provider staging bytes as user-owned cleanup candidates.
6. Files inside a `.photoslibrary`/`.photolibrary` bundle are non-overridable
   `system-managed-photos-library-data` blockers; individual SQLite members are never copied.
7. Only a fresh attestation plus the separate receipt-bound human approval may move the source to
   the OS Trash. The destination and Trash are never emptied by DiskSage.
8. Keep DiskSage repositories, Git worktrees, and temporary evidence outside macOS-managed
   File Provider roots (for example `~/Documents` when it carries a provider-domain marker).
   A dataless `.git` file or a Git operation that waits on materialization is provider evidence,
   not a stale-worktree deletion signal; stop the audit and relocate the worktree to a local
   volume such as `/private/tmp` before continuing.

9. Regenerable development artifacts (`target`, `node_modules`, virtual environments,
   `__pycache__`, and `.codegraph` indexes) can be inventoried headlessly with
   `disksage-dev-artifacts --root ABSOLUTE_PATH --min-age-days N`. The default is read-only;
   `--execute --journal-path ABSOLUTE_PATH` performs a fresh bounded manifest and moves only
   unchanged, identity-matching artifacts to OS Trash. Protected/system paths and incomplete
   manifests fail closed. This command never writes a cloud provider or authorizes source
   eviction; do not use it to remove a provider-managed File Provider tree.

10. For archive lineage, inspect embedded content before trusting the archive filename or central
    directory timestamp. ZIP `.eml` headers are streamed with bounded Rust reads and their RFC 5322
    `Date` is preferred when the inner scan is complete; a bounded or malformed inner scan creates
    a metadata warning and keeps the candidate blocked.

11. To investigate macOS application residue, use `관계 기반 고아 정리` to create a bounded,
    metadata-only plan. The plan compares installed bundle IDs with `Library/Caches` and
    `Library/Application Support`; it exposes no local paths. Treat `Application Support`,
    incomplete scans, skipped entries, or active-use evidence as review-only. Only candidates
    explicitly marked `완전 스캔·미사용 캐시` may be selected. The exact approval phrase and a
    non-empty rationale are required; execution re-plans and moves only unchanged cache candidates
    to OS Trash. This does not touch cloud providers or empty Trash. If the plan is incomplete,
    investigate provider/File Provider activity first and rerun after the provider is quiet.

The Goal and ADR files are replaceable projections. Agents or operators must compare them with the
immutable receipt/evidence record before any mutation. Naruon receives lineage/provider evidence,
not a second independent deletion authority.

## Standards basis (APA 7)

The File Provider boundary follows Apple's distinction between dataless metadata and materialized
content, and its requirement that working-set changes be enumerated and synchronized; a quiet
Finder window therefore cannot substitute for per-item provider evidence (Apple, n.d.-a, n.d.-b).
Lineage exports use PROV-O entities, activities, and relations, while catalog projections use DCAT
3 dataset and distribution concepts (Albertoni et al., 2024; Lebo et al., 2013). The access,
integrity, and audit boundaries are aligned with NIST SP 800-53 Rev. 5 (Joint Task Force, 2020).

### References

Albertoni, R., Browning, D., Cox, S. J., Gonzalez Beltran, A., Perego, A., & Winstanley, P.
(Eds.). (2024). *Data Catalog Vocabulary (DCAT) - Version 3*. W3C Recommendation.
https://www.w3.org/TR/vocab-dcat-3/

Apple. (n.d.-a). *Defining your File Provider's content*. Apple Developer Documentation.
Retrieved August 21, 2026, from https://developer.apple.com/documentation/FileProvider/defining-your-file-provider-s-content

Apple. (n.d.-b). *Synchronizing the File Provider extension*. Apple Developer Documentation.
Retrieved August 21, 2026, from https://developer.apple.com/documentation/fileprovider/synchronizing-the-file-provider-extension

Joint Task Force. (2020). *Security and privacy controls for information systems and organizations*
(NIST Special Publication 800-53, Revision 5). National Institute of Standards and Technology.
https://doi.org/10.6028/NIST.SP.800-53r5

Lebo, T., Sahoo, S., & McGuinness, D. (Eds.). (2013). *PROV-O: The PROV ontology*.
W3C Recommendation. https://www.w3.org/TR/prov-o/
