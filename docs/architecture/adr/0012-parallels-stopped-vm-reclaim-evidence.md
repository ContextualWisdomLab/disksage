# ADR-0012: Parallels stopped-VM reclaim requires native, fresh execution evidence

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

DiskSage ships a macOS-only opt-in planner and compact executor. Planning runs bounded read-only native commands, requires
an exact stopped VM record, rejects symlinks and cloud-provider roots, binds canonical `.pvm` bundle
and contained disk identities, performs a bounded allocation walk, and requires complete inactive
`lsof` evidence. Reclaimable bytes are calculated only from native `Block size`, `Allocated blocks`,
and `Used blocks` fields. Missing or inconsistent evidence fails closed.

Execution additionally requires authoritative empty snapshot JSON, a nonzero native compact
estimate, an exact approval phrase with human attribution and rationale, and a plan and approval no
older than five minutes. It re-observes registration, stopped state, snapshots, active use, bundle
and disk identity, and logical and physical allocation immediately before invoking the fixed vendor
command `prl_disk_tool compact -hdd <approved-disk>`. Callers cannot add `--force` or select another
executable. Approval and result records are create-new local JSON; the result distinguishes the disk
allocation reduction from the host volume's observed free-space change. Read-only probes retain a
30-second deadline; the isolated compact process group has a separate seven-day ceiling so a large
disk is not killed by the probe deadline. Once mutation is attempted, command failure and bounded
post-operation observations are still returned for immutable recording, and the CLI exits nonzero
only after emitting that receipt. Any pre-execution drift aborts before the native mutation.
DiskSage never deletes snapshots, stops a VM, moves a bundle, or claims the estimate as recovered
capacity.

## References

Parallels International GmbH. (2022). *Parallels Desktop for Mac Business and Pro Editions:
Command-line reference* (Version 20). https://download.parallels.com/desktop/v20/docs/en_US/Parallels%20Desktop%20Command-Line%20Reference.pdf

Parallels International GmbH. (2023). *Parallels Desktop for Mac Business and Pro Editions:
Command-line reference* (Version 18). https://download.parallels.com/desktop/v18/docs/en_US/Parallels%20Desktop%20Command-Line%20Reference.pdf

Parallels International GmbH. (2024). *What is an expanding disk?* Parallels Knowledge Base.
https://kb.parallels.com/en/4706
