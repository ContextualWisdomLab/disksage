# ADR-0012: Keep Colima disk reclaim behind native stopped-VM support

- Status: Accepted
- Date: 2026-08-30

## Context

Colima sparse backing disks can retain host allocation after container data is removed. Colima's
official FAQ documents `colima ssh -- sudo fstrim -a`, which requires a running guest. The current
Colima command inventory has no native stopped-VM compact command. Lima's instance `diffdisk` is a
VM implementation detail; direct deletion, truncation, `qemu-img` conversion, or an inferred Lima
command would cross the provider's supported boundary.

## Decision

DiskSage provides a read-only Rust plan that uses bounded `colima list --json` evidence, validates
the profile name and macOS Colima storage layout, rejects symlinked/untrusted backing disks, and
records logical bytes, allocated bytes, filesystem identity, runtime state, VM type, and runtime.
An already stopped VM proves that no guest workload is active. A running or unknown VM blocks the
plan.

Execution is disabled by default and remains unavailable even after a fresh exact approval because
the current provider offers no stopped-VM native compact command. The receipt truthfully reports
zero execution and unknown physical reclaim. DiskSage never invokes `colima stop`, guest `fstrim`,
`qemu-img`, raw-disk deletion/truncation, or an undocumented Lima operation. If Colima later ships a
documented native stopped-VM compact command, a new ADR and versioned execution contract must bind
the exact command, profile, executable identity, backing-disk identity, and post-operation evidence.

## Consequences

Customers can see why host allocation remains high and what to do next without risking a running
workload or corrupting a VM disk. Current installations receive an explicit unavailable receipt
rather than a false reclaim claim.

## Rejected alternatives

- Starting or stopping Colima automatically changes runtime state without operator intent.
- Guest `fstrim` cannot satisfy the stopped-VM execution invariant.
- Direct raw-disk or `qemu-img` operations are not a supported Colima product boundary.

## References

Abiosoft. (2026). *Frequently asked questions: How can disk space be recovered?* Colima. https://github.com/abiosoft/colima/blob/main/docs/FAQ.md

Abiosoft. (2026). *List instances command* [Source code]. Colima. https://github.com/abiosoft/colima/blob/main/cmd/list.go

Lima Authors. (2025). *Document how to increase disk size* (Issue No. 3418). GitHub. https://github.com/lima-vm/lima/issues/3418
