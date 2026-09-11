# DiskSage standards and evidence traceability

**Status:** Living engineering evidence map

**Snapshot:** 2026-09-12

This document records why external standards and primary technical authorities constrain DiskSage behavior. It does not turn a cited source, an open pull request, or a test fixture into shipped evidence. Runtime code, exact-head tests, protected Git history, immutable release artifacts, and recovery receipts remain the acceptance evidence.

## Filesystem publication and race safety

### Connection-document publication — issue #342 / PRs #344 and #339

**Problem.** `save_connections()` on #339 validates a pathname and later creates and renames by pathname. An ancestor can therefore be replaced between check and use. The current domain implementation also synchronizes the temporary file but does not explicitly synchronize the containing directory after publication.

**Constraints.** Validation authority and mutation authority must remain bound to the same directory object. On POSIX systems, temporary creation, replacement, and cleanup are descriptor-relative after the final parent is pinned; a second pathname check is not equivalent. Namespace durability is separate from atomic rename. On Windows, final publication must use pinned native namespace authority, and temporary creation must not remain redirectable merely because the final rename is handle-relative.

**Accepted design direction.** #344 owns the reusable foundation. Its current Unix primitive opens/pins the final directory, creates a create-new temporary record with `openat`, keeps cleanup descriptor-relative with `unlinkat`, replaces with `renameat`, synchronizes file data before replacement, and synchronizes the containing directory after replacement. Real temporary-filesystem hooks replace the visible parent before temporary creation, after temporary sync, and after rename. #339 remains responsible for consuming that primitive and mapping its failure states into provider-OAuth domain semantics. Windows intentionally has no pathname fallback; native handle-bound temporary creation/replacement parity remains open.

**Rejected alternatives.** Re-running `symlink_metadata()` immediately before rename, shrinking the race window, retry loops, broad permission changes, or pathname-only cleanup do not bind the checked object to the mutated object and therefore do not close CWE-367.

**Exact acceptance evidence.** Foundation and domain evidence are both required. A real temporary filesystem fixture replaces visible parent A with B (1) before temporary creation and (2) after temporary-file sync but before final publication. Connection JSON must never be redirected into B. Existing destination symlink/reparse-point and non-regular-file cases remain fail closed. Successful publication must remain loadable, preserve Unix 0600 leaf permissions, and distinguish file-data synchronization from namespace synchronization. A post-publication namespace change is reported separately because publication to the already-admitted directory object may have occurred.

**Durability scope.** POSIX.1-2024 explicitly distinguishes directory-operation atomicity from persistence of the modified directory entry: an application that needs the new entry to be durable synchronizes the directory. On Apple platforms, stronger primitives such as `F_FULLFSYNC` have different cost and guarantee boundaries from ordinary `fsync`; DiskSage must name and test the primitive it actually relies on rather than using “atomic” and “durable” interchangeably.

**Research rationale.** Bishop and Dilger (1996) characterize file-access races as failures in which a pathname's name-to-object binding changes between repeated references. That supports treating a repeated pathname check as observation, not as mutation authority. Tsafrir et al. (2008) show that timing/probability-based TOCTTOU defenses can be defeated by adversarial filesystem structures and synchronization. DiskSage therefore uses those papers to justify the threat model and adversarial fixture design, while POSIX and platform APIs remain authoritative for the actual mitigation contract. Neither paper is used to claim that a particular DiskSage implementation is race-free without exact-head filesystem evidence.

### Git-worktree deletion authority — PR #337 / historical donor #156

Git `prunable` metadata, incomplete size/status evidence, dirty state, active process use, and truncated process evidence are not deletion authority. Current owner tests use real temporary Git repositories and linked worktrees. A stale registration whose worktree directory disappeared is an `EvidenceGap`; a dirty or actively used worktree is preserved; none of these read-only audits executes filesystem mutation.

This boundary is intentionally stronger than a synthetic object-state unit test because deletion safety depends on Git registration state, filesystem presence, process state, and bounded evidence being observed together.

## Unix subprocess and process-group lifecycle

### Identity-preserving bounded execution — issues #385/#393 and PRs #386/#399

**Problem.** Several Unix adapters create a private process group so timeout or descendant-held pipes can be bounded. Reaping the direct process-group leader before all group-directed signals are complete removes the live/waitable identity anchor. A later `kill(-pgid, ...)` still targets a numeric process-group ID; if the prior group lifetime has ended and the number has been reused, the signal can address an unrelated group. Separately, blocking stdout/stderr readers created before child observation can outlive an error return when descendants retain the pipe write ends.

**Constraints.** POSIX.1-2024 `waitid()` with `WNOWAIT` obtains status without consuming it, so an exited child can remain waitable until the caller deliberately reaps it. `WNOHANG` permits a nonblocking observation. An `EINTR` result says the operation was interrupted by a signal; it does not itself consume a child wait status, so DiskSage retries that observation. Other observation errors are not treated as proof that the old numeric PID/PGID remains a safe signalling target. POSIX `kill()` with a negative `pid` selects a process group by the absolute numeric value. POSIX.1-2024 also defines process-ID/process-group-ID lifetimes and explicitly discusses why reusing an old process-group number can misdirect later signals. DiskSage's rule—keep the leader live or waitable until group signalling is finished—is an engineering consequence of these primitives and lifetime rules, not a claim that POSIX prescribes DiskSage's exact wrapper design.

**Accepted design direction.** #386 is the canonical Unix lifecycle owner. Normal exit is observed without reaping; descendants are settled while the leader still pins identity; `Child::wait()` then consumes status exactly once. Timeout cleanup likewise signals the verified private group before reaping. Interrupted no-reap observations are retried only for `EINTR`; a genuine non-interrupted observation failure fails closed and cannot authorize a negative-PID signal. Domain adapters remain responsible for bounding or cancelling their own reader threads/file descriptors and for translating uncertainty into domain evidence rather than detaching resources or manufacturing success.

**Rejected alternatives.** `try_wait()` followed by remembered-PGID signalling, retrying every observation error, detaching blocked readers, widening command timeouts, or killing an unverified process group are rejected. Those approaches either lose identity continuity, conceal uncertainty, or leave DiskSage-owned resources outside the bounded execution contract.

**Exact acceptance evidence.** The canonical foundation requires real Unix child/process-group fixtures for running, exited-but-unreaped, timeout, descendant-held-pipe, interruption retry, explicit final reap, and fail-closed observation errors. Each consumer must then prove its own fixed executable/arguments, domain evidence, output bounds, and platform behavior on a resulting exact head. #399 exact `1d49d9faad26de2bbc26a605a578d35804eee009` has terminal Test `34642960506` SUCCESS for its original success/timeout descendant-held-pipe repair, but current review still requires bounded/cancellable reader cleanup for genuine non-interrupted observation failure. #386 exact `ad61c380ee3cb0319bf423afe1247f91cce033ce` adds EINTR-only retry and must earn its own terminal exact-head evidence before dependent restack.

## Ontology and provenance boundary

DiskSage filesystem classification uses ontology terms as semantic evidence, not as mutation permission. OWL 2 remains the formal ontology-language reference. SHACL is the validation reference for RDF graph constraints where a shape contract is used. PROV-O is the provenance vocabulary reference when an evidence artifact needs explicit entity/activity/agent provenance. None of these vocabularies authorizes deletion, remote synchronization, or credential use by itself.

Ontology labels remain separate from translated UI resources. A localized label may change presentation; it must not change an ontology identifier, filesystem invariant, reclaim policy, or evidence fingerprint.

## Security mapping

Issue #342 maps to CWE-367 (Time-of-check Time-of-use Race Condition): a resource property is checked, the resource can change before use, and later pathname-based mutation may act on a different object. The mitigation requirement is object/namespace authority continuity, not merely a faster second check.

## Evidence-to-owner matrix

| External authority | DiskSage decision | Canonical owner | Required repository evidence |
| --- | --- | --- | --- |
| POSIX.1-2024 `renameat()` and *at-family rationale | Use opened directory authority to avoid pathname-component replacement races on POSIX publication | #344 foundation; #339/#342 consumer | Real ancestor-replacement RED→GREEN fixture; descriptor-relative create/rename/cleanup; domain adoption |
| POSIX.1-2024 directory durability rationale | Atomic rename does not by itself justify a durable-new-entry claim | #344 foundation; #339/#342 consumer | File sync plus containing-directory sync and explicit failure/uncertainty contract |
| Bishop & Dilger (1996) | Treat repeated pathname references as vulnerable when the name-to-object binding can change | #344 foundation; #339/#342 consumer | Object-bound authority plus deterministic namespace-replacement fixtures; no “second check” completion claim |
| Tsafrir et al. (2008) | Do not rely on shrinking a TOCTTOU race window or probabilistic retry as the security boundary | #344 foundation; #339/#342 consumer | Stable namespace authority under adversarial scheduling/structure; platform contract remains primary |
| Microsoft `FILE_RENAME_INFO` / `SetFileInformationByHandle` | Relative rename may resolve against `RootDirectory`; temporary creation still needs pinned namespace authority | #339 / issue #342 | Windows reparse/ancestor replacement fixture and native-handle creation/replacement evidence |
| Apple `fsync(2)` / `fcntl(F_FULLFSYNC)` documentation | macOS persistence language must identify the primitive actually used and not overstate crash/power-loss guarantees | #344 foundation / #339 consumer | macOS filesystem fixture plus documented ordinary-vs-stronger sync decision, fallback, and cost |
| CWE-367 | Pathname check/use gaps are a security weakness even without a final-component symlink | #339 / SECURITY/THREAT_MODEL | Causal test and stable-authority fix, not timing reduction |
| POSIX.1-2024 `waitid(..., WNOWAIT)` | Observe exited children without consuming wait status so the leader can remain the lifecycle identity anchor until group cleanup is complete; retry only signal interruption | #386 foundation; #385 migration coordinator | Real running/exited-unreaped/timeout/EINTR fixtures, explicit final reap, no `try_wait()` fallback |
| POSIX.1-2024 `kill()` negative-pid process-group selection + process-ID reuse rules | Do not send a group signal after the numeric identity is no longer proven to refer to the admitted private group | #386 foundation; #393/#399 and other domain consumers | Group signal occurs while leader is live/waitable; genuine observation failure sends no unverified negative-PID signal |
| POSIX.1-2024 read/interruption semantics | Pipe readers must handle interruption and remain bounded without converting an error path into detached resource ownership | each subprocess domain adapter | Real descendant-held stdout/stderr plus error-path cleanup fixture; no timeout widening or detached-reader acceptance |
| OWL 2 | Formal classes/properties/individuals may express filesystem classification semantics | ontology/classification owner | Ontology conformance plus mapping tests; no mutation-authority inference |
| SHACL | RDF graph validity may be checked against explicit shapes | ontology validation adapter | Fail-closed validation fixtures for required shapes |
| PROV-O | Provenance relationships may describe evidence production/derivation | evidence/provenance adapter | Stable identifiers and provenance-preservation tests; no deletion authorization |

## References

Apple Inc. (n.d.). *Mac OS X manual page for fcntl(2).* Apple Developer Documentation. https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fcntl.2.html

Apple Inc. (n.d.). *Mac OS X manual page for fsync(2).* Apple Developer Documentation. https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fsync.2.html

Bishop, M., & Dilger, M. (1996). Checking for race conditions in file accesses. *Computing Systems, 9*(2), 131–152. https://nob.cs.ucdavis.edu/bishop/papers/1996-compsys/

Microsoft. (2026, May 15). *FILE_RENAME_INFO structure (winbase.h).* Microsoft Learn. https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info

Microsoft. (2021, October 13). *SetFileInformationByHandle function (fileapi.h).* Microsoft Learn. https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle

MITRE. (2026, April 30). *CWE-367: Time-of-check Time-of-use (TOCTOU) race condition (CWE 4.20).* Common Weakness Enumeration. https://cwe.mitre.org/data/definitions/367.html

The Open Group. (2024). *General concepts: Process ID reuse.* In *The Open Group Base Specifications Issue 8, IEEE Std 1003.1-2024.* https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap04.html

The Open Group. (2024). *kill — send a signal to a process or a group of processes.* In *The Open Group Base Specifications Issue 8, IEEE Std 1003.1-2024.* https://pubs.opengroup.org/onlinepubs/9799919799/functions/kill.html

The Open Group. (2024). *wait, waitpid, waitid — wait for a process to change state.* In *The Open Group Base Specifications Issue 8, IEEE Std 1003.1-2024.* https://pubs.opengroup.org/onlinepubs/9799919799/functions/wait.html

The Open Group. (2024). *read — read from a file.* In *The Open Group Base Specifications Issue 8, IEEE Std 1003.1-2024.* https://pubs.opengroup.org/onlinepubs/9799919799/functions/read.html

The Open Group. (2024). *Rationale for Base Definitions: Directory operations and durability.* In *The Open Group Base Specifications Issue 8, IEEE Std 1003.1-2024.* https://pubs.opengroup.org/onlinepubs/9799919799/xrat/V4_xbd_chap01.html

The Open Group. (2024). *Portability considerations: race-free and thread-safe file access.* In *The Open Group Base Specifications Issue 8, IEEE Std 1003.1-2024.* https://pubs.opengroup.org/onlinepubs/9799919799/xrat/V4_port.html

Tsafrir, D., Hertz, T., Wagner, D. A., & Da Silva, D. (2008). Portably solving file races with hardness amplification. *ACM Transactions on Storage, 4*(3), Article 9. https://doi.org/10.1145/1416944.1416948

World Wide Web Consortium. (2012, December 11). *OWL 2 Web Ontology Language: Document overview (Second Edition).* https://www.w3.org/TR/owl-overview/

World Wide Web Consortium. (2013, April 30). *PROV-O: The PROV ontology.* https://www.w3.org/TR/prov-o/

World Wide Web Consortium. (2017, July 20). *Shapes Constraint Language (SHACL).* https://www.w3.org/TR/shacl/

## Source-management note

The standards and research above are publicly accessible primary/publisher/author sources. This snapshot does not claim that a local Zotero library contains them. If a Zotero-backed research ledger is later attached, cite its stable local item keys in addition to—not instead of—the canonical standards and publication URLs.
