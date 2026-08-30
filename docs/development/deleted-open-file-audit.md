# Deleted-open file audit

When a running process keeps a file open after its final directory entry is removed, POSIX defers
freeing the file contents until the remaining references are closed. This can make a cleanup look
complete while local capacity remains occupied.

`disksage-deleted-open-audit` runs a bounded, read-only `lsof +L1` observation. It:

- deduplicates files by observed device and inode identity;
- reports the observed logical file size separately from physical reclaim, which remains unknown;
- retains no deleted pathname and performs no process or filesystem mutation;
- caps captured output and record count, and fails evidence completeness closed;
- tells the person to close the listed apps normally and scan again.

The audit never kills a process, removes a file, or counts its logical total toward verified
physical recovery. A later APFS observation remains a separate shared-volume measurement.

The Cleanup screen exposes that same boundary as a read-only action plan. It groups holders by
application name, presents logical bytes only as capacity still held, and keeps the bounded receipt
identifier under audit details. DiskSage neither offers nor invokes a forced quit. After the person
quits every listed instance normally, scanning again creates a fresh receipt; only the separate
APFS available-space observation may describe physical recovery.

## Reference

The Open Group. (2024). *unlink, unlinkat*. In *POSIX.1-2024*. IEEE and The Open Group.
https://pubs.opengroup.org/onlinepubs/9799919799/functions/unlink.html
