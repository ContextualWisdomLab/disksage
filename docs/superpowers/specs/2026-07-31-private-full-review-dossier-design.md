# Private full-plan review dossier

## Problem

The current exact-reason-set review dossier is appropriate for applying one
bounded human decision policy to a homogeneous group. A real Downloads plan can
contain many distinct review-reason sets plus candidates that are blocked before
review. Re-running the complete metadata and archive scan once for every reason
set is slow, creates avoidable build and probe work, and hides blocked candidates
from the operator's complete organization picture.

The full raw `CloudPlanReport` contains the necessary evidence, but writing
stdout to a file does not provide DiskSage's create-new, regular-file, mode-0600,
bounded-output guarantees.

## CLI contract

`disksage-cloud-plan` adds:

```text
--decision-summary
--private-full-review-output /absolute/new-file.json
```

The flag is valid only for one selected cloud destination. It is mutually
exclusive with `--all-readable-roots`, `--review-reason-set`, and the existing
exact-slice `--private-review-output`. The destination output path must be
absolute, its existing parent must be a real directory rather than a symlink,
and the target must not already exist.

The private file uses the existing bounded writer:

- create-new only;
- regular file, not a symlink;
- Unix mode 0600;
- file and parent-directory synchronization;
- 16 MiB maximum serialized size; and
- cleanup of a partially written target on failure.

## Private dossier

The `private-full-review-dossier` contains every candidate in the fresh plan,
including blocked candidates. Candidates are deterministically ordered by
metadata and review fingerprint. It retains:

- absolute source and proposed destination paths;
- relative path and source context;
- selected production time, source, confidence, and raw metadata evidence;
- content title, author, contextual and dataset evidence;
- candidate and review fingerprints;
- exact duplicate evidence, capacity assessment, and plan notices; and
- path-free decision, metadata-source, archive-kind, month, destination-bucket,
  review-reason, and blocker aggregates.

Blocked candidates remain in the dossier for diagnosis but are explicitly not
approval-eligible until their blocker is resolved. Review-required candidates
still require individual attributed approve or hold decisions bound to the
candidate's current review fingerprint.

## Public stdout summary

When the private full dossier is written, stdout switches to
`private-full-review-summary`. It contains only path-free aggregates, provider
and account-scope enums, the decision-batch fingerprint, capacity state, notices,
and the private file's byte length and SHA-256. It omits every path, relative
name, title, author, raw metadata value, dataset profile, and duplicate member.

The SHA-256 is an integrity checksum for the local private file, not a signature
or approval. The summary fixes provider sync, cloud write, and source eviction
authority to false.

## Metadata and trust policy

Production-time precedence remains:

1. embedded metadata;
2. explicit filename date;
3. filesystem creation time; and
4. filesystem modification time.

Filename dates remain auxiliary even when selected. A quiet provider client,
available destination path, or written dossier does not prove account
authentication, remote capacity, synchronization, upload completion, or safe
local eviction.

## Integration boundary

The private dossier remains local and must not be submitted to Naruon because it
contains paths and raw user metadata. Naruon already accepts the separate
path-free cloud-copy readiness envelope and can validate aggregate consistency
without receiving this sensitive evidence. No database schema, semantic portal,
agent, LLM, or judge is needed for this deterministic local review surface.
