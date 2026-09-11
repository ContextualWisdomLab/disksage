# ADR-0004: Bound fixed maintenance command execution

**Status:** Accepted  
**Date:** 2026-08-20  
**Amended:** 2026-09-12

## Context

DiskSage can execute the fixed macOS command `brew cleanup --prune-prefix` only
after a bounded dry-run, local-model judgment, fast-mlsirm calibration, and
separate human confirmation. A failed execution gate must not silently leave
owned reader threads or file descriptors running beyond the command boundary.
Descendants can inherit stdout/stderr and can outlive the direct process-group
leader, so reaping that leader before group cleanup loses the live/waitable PID
identity that makes a later negative-PID signal safe.

A `waitid(..., WNOWAIT)` interruption (`EINTR`) is not evidence that the child
identity was lost: the observation can be retried without consuming the wait
status. A genuine non-interrupted observation failure is different. Once the
wrapper can no longer prove that the numeric leader still anchors the private
process group, signalling `-PGID` risks targeting a reused, unrelated process
group.

## Decision

Run the verified Homebrew wrapper in a private Unix process group and keep the
direct leader live or exited-but-unreaped until every process-group-directed
signal is complete.

- Normal exit is observed without reaping. Settle descendants while the leader
  identity is still pinned, then reap the leader exactly once and join bounded
  stdout/stderr readers.
- Timeout is observed while the leader is still running. Terminate the verified
  private process group, reap the leader, then join the bounded readers.
- Retry only interrupted no-reap observations. A genuine observation error
  fails closed: do not send a negative-PID signal to an unverified group. Bound
  or cancel DiskSage-owned readers and descriptors, reap or terminate only the
  direct child when that remains safe, and report that descendant cleanup could
  not be proven instead of manufacturing success.

The executable path, arguments, plan fingerprint, model judgment, calibration
result, approval phrase, and audit record remain independently validated. No
model output can provide a command, path, process identity, or cleanup
authority.

## Alternatives rejected

- **Reap first, signal the remembered PGID later.** Rejected because PID/PGID
  reuse can redirect a negative-PID signal to an unrelated process group.
- **Treat every `waitid` error as retryable.** Rejected because errors such as
  `ECHILD` do not prove that the old numeric group identity is still safe.
- **Kill the direct child and detach blocking reader threads.** Rejected because
  descendants retaining pipe write ends can keep DiskSage-owned threads and
  descriptors alive outside the bounded execution contract.
- **Increase timeouts or retries.** Rejected because it changes latency without
  repairing identity or ownership of the resources that can outlive the bound.

## Consequences

- Success and timeout cleanup preserve a concrete identity anchor until group
  signalling is complete.
- Observation failure remains fail closed even when that means DiskSage cannot
  safely terminate an unverified descendant group.
- Output capture is part of lifecycle ownership: reader threads and descriptors
  require bounded/cancellable cleanup rather than detach-on-error behavior.
- The source tree and cloud-provider state are unaffected by this local-only
  maintenance action.
- A timeout or observation failure is recorded as failure; neither is treated
  as successful cleanup.
- The private process-group setup is Unix-specific and requires current macOS
  runtime evidence for the shipped Homebrew adapter.

## Implementation and evidence status

- `src-tauri/src/unix_process_group.rs` is the canonical reusable lifecycle
  owner. PR #386 current exact `ad61c380ee3cb0319bf423afe1247f91cce033ce`
  adds EINTR-only retry while retaining fail-closed handling for other
  observation errors; its resulting-head Test must be terminal before consumer
  adoption is claimed.
- `src-tauri/src/brew_cleanup.rs` is the Homebrew domain adapter. Its current
  protected-lineage implementation still requires the #206/#385 migration to
  consume the verified canonical lifecycle and prove a real descendant-held-
  pipe fixture. This ADR does not turn that open gap into runtime evidence.
- Git-worktree issue #393/#399 independently exposed the same identity and
  reader-lifecycle distinction. Its evidence is useful for the shared process
  mechanic but does not grant Homebrew mutation authority.
