# ADR-0012: Parallels stopped-VM reclaim evidence remains read-only

**Status:** Accepted
**Date:** 2026-08-30

## Context

A local observation measured roughly 103 GiB in `~/Parallels`, but the installed native disk tool
estimated only about 48 MiB as compactable. DiskSage must not infer reclaim from bundle size.
Parallels documents `prlctl list -a -j` as the all-VM JSON inventory and documents
`prl_disk_tool compact --info --hdd` as the non-mutating estimate whose allocated and used block
counts determine the possible reduction. Parallels also documents that snapshots can prevent
compaction and that compacting removes empty blocks from expanding disks.

## Decision

DiskSage ships a macOS-only opt-in planner. It runs only bounded read-only native commands, requires
an exact stopped VM record, rejects symlinks and cloud-provider roots, binds canonical `.pvm` bundle
and contained disk identities, performs a bounded allocation walk, and requires complete inactive
`lsof` evidence. Reclaimable bytes are calculated only from native `Block size`, `Allocated blocks`,
and `Used blocks` fields. Missing or inconsistent evidence fails closed.

The current boundary cannot compact, delete, stop a VM, or move a bundle. Even a nonzero estimate
returns `execution_available=false` and tells the customer to keep the VM unchanged. A future
mutation ADR must identify a supported installed version, re-observe stopped state and every
identity immediately before execution, require fresh exact human approval, and verify post-state.

## References

Parallels International GmbH. (2022). *Parallels Desktop for Mac Business and Pro Editions:
Command-line reference* (Version 20). https://download.parallels.com/desktop/v20/docs/en_US/Parallels%20Desktop%20Command-Line%20Reference.pdf

Parallels International GmbH. (2023). *Parallels Desktop for Mac Business and Pro Editions:
Command-line reference* (Version 18). https://download.parallels.com/desktop/v18/docs/en_US/Parallels%20Desktop%20Command-Line%20Reference.pdf

Parallels International GmbH. (2024). *What is an expanding disk?* Parallels Knowledge Base.
https://kb.parallels.com/en/4706
