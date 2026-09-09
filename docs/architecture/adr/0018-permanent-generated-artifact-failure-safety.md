# ADR-0018: Retain failed permanent generated-artifact deletions in private staging

- Status: Accepted
- Date: 2026-08-29

## Context

An explicitly approved permanent cleanup may move a regenerated development artifact into a private
sibling staging directory before recursive deletion. Recursive deletion is not atomic: a filesystem
error can leave only part of the staged tree removed. Restoring that partial tree to its original
path would present damaged generated state as a live artifact.

## Decision

After the identity-bound staging move, a permanent deletion failure retains the remaining staged
tree in the private staging directory and returns a failure. DiskSage never restores a partially
removed tree to the original path. The journal records the pending operation and terminal error;
only a complete recursive deletion is successful. The normal Trash path remains reversible.

## Consequences

- A failed permanent cleanup cannot replace a live path with a partial generated tree.
- Remaining staged bytes stay available for forensic recovery or regeneration until explicitly handled.
- The private staging location is not success evidence and never authorizes a cloud or user-file action.

## Alternatives rejected

Restoring after `remove_dir_all` fails is rejected because the directory may already be partial.
Deleting the staging directory on failure is rejected because it discards the remaining recovery data.

## Evidence

`src-tauri/src/safety.rs` rechecks filesystem identity before staging, journals pending and terminal
outcomes, and tests that a simulated partial recursive-delete failure leaves the partial tree staged.
