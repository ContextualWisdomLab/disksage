# Local duplicate canonical-selection decisions

`disksage-duplicate-canonical-review` turns a secure local canonical-review dossier into either:

- a redacted, read-only verification summary; or
- one integrity-bound human decision selecting a canonical member or holding the cluster.

The command never moves, copies, trashes, deletes, or uploads a source file. A `selected` decision
records which member should be preserved as canonical; it does not authorize discarding any other
member. A `held` decision records that more context is required.

## Verify a dossier

The dossier must be an absolute, regular, non-symlink JSON file no larger than 8 MiB. On Unix it
must not be readable or writable by group or other users.

```sh
cargo run --features cloud-cli --bin disksage-duplicate-canonical-review -- \
  --dossier /absolute/private/review-dossier.json \
  --verify
```

Verification checks the canonical dossier ID and aggregate arithmetic, enforces the production-time
precedence `embedded metadata → explicit filename date → filesystem created → filesystem modified`,
and revalidates every candidate's regular-file status, size, modified time, and containment beneath
the source root. Its stdout summary contains no local paths or metadata values.

## Record a canonical selection

The operator must inspect the path-bearing dossier and provide a `human:` reviewer identity and
bounded rationale. Agent identities cannot use the human namespace.

```sh
cargo run --features cloud-cli --bin disksage-duplicate-canonical-review -- \
  --dossier /absolute/private/review-dossier.json \
  --cluster-ref CLUSTER_HEX64 \
  --disposition selected \
  --selected-member-ref MEMBER_HEX64 \
  --reviewed-by human:owner \
  --rationale "Embedded metadata and file context were manually reviewed." \
  --decision-dir /absolute/private/existing-decisions
```

To defer a cluster, use `--disposition held` and omit `--selected-member-ref`. The decision is
created once with mode `0600` outside the audited source root and is bound to:

- the dossier, canonical-review lineage, and duplicate-audit lineage IDs;
- the exact cluster and complete opaque member set;
- the recommendation and the selected member, if any;
- the human attribution, rationale, and review timestamp.

The selected cluster is revalidated immediately before the decision is written. A changed,
missing, symlinked, or out-of-root member fails closed. Every decision explicitly sets
`discard_authorization`, `mutation_performed`, and `cloud_write_performed` to `false`.
