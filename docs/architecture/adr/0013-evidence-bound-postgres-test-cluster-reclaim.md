# ADR-0013: PostgreSQL test-cluster reclaim requires an exact allowlist

- **Status:** Accepted
- **Date:** 2026-08-29

## Decision

DiskSage may reclaim an operator-identified PostgreSQL test cluster only through the Rust
`disksage-postgres-test-reclaim` CLI. Planning is the default and performs no shutdown or removal.
It requires an absolute data directory, explicit native `psql` and `pg_ctl` paths, a database user,
and one or more repeated `--expected-database` values. DiskSage never infers authority from a
directory/database name, modification time, parent PID, or an LLM judgment.

The plan must prove all of the following at once:

1. the canonical data directory is owned by the current user, mode-private, symlink-free, and has
   the PostgreSQL `PG_VERSION`, `base`, `global`, `pg_wal`, and ready `postmaster.pid` structure;
2. the live postmaster PID exists and the PID-file data directory, port, and socket directory bind
   the native observations to that cluster;
3. the sorted non-template, non-default database set equals the explicit allowlist exactly;
4. `pg_stat_activity` reports zero other `client backend` sessions; and
5. the supplied `psql` and `pg_ctl` are absolute, regular, non-symlink executable objects whose
   filesystem identities enter the plan fingerprint and are checked around native execution.

Execution requires the exact fingerprint and exact approval phrase, then repeats the complete
plan. It creates a mode-0600 pending journal before running bounded native `pg_ctl -m fast -w
stop`, confirms the postmaster PID is gone, rechecks the data-directory inode, removes only that
directory, records the `statvfs` available-space delta, and writes a separate immutable result.
Public JSON contains no local path; exact paths remain only in private evidence. Any missing,
changed, ambiguous, timed-out, nonzero-client, journal, identity, shutdown, or filesystem evidence
fails closed.

## Rationale and consequences

PostgreSQL documents `pg_ctl -w stop` as the native controlled lifecycle and defines fast shutdown
as rollback plus client disconnection rather than crash recovery. DiskSage still requires zero
external clients before invoking it, using the documented `pg_stat_activity.backend_type = 'client
backend'` evidence. Pending/result audit records and exact change authorization align the operation
with NIST audit, accountability, and configuration-management control families. This feature does
not authorize production clusters, provider data, shared clusters, inferred test databases, or
automatic background deletion.

## References

National Institute of Standards and Technology. (2020). *Security and privacy controls for
information systems and organizations* (NIST Special Publication 800-53, Revision 5, Update 1).
https://doi.org/10.6028/NIST.SP.800-53r5

PostgreSQL Global Development Group. (2026a). *PostgreSQL 18 documentation: pg_ctl*.
https://www.postgresql.org/docs/18/app-pg-ctl.html

PostgreSQL Global Development Group. (2026b). *PostgreSQL 18 documentation: Monitoring database
activity*. https://www.postgresql.org/docs/18/monitoring-stats.html
