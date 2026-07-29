# Maven local repository provenance audit

`disksage-maven-cache-audit` is a read-only Rust command for separating Maven
version directories that have explicit remote provenance from directories that
may contain locally installed or otherwise irreplaceable artifacts.

```bash
cargo run --manifest-path src-tauri/Cargo.toml \
  --bin disksage-maven-cache-audit -- \
  --repository-root "$HOME/.m2/repository" \
  --output "/absolute/new/private-report.json"
```

The `disksage.maven-cache-audit/v1` report treats a version directory as
`remote_recoverable` only when all of the following hold:

- `_remote.repositories` is a bounded UTF-8 regular file;
- every artifact payload is attributed to a non-empty repository ID;
- every marker reference still exists;
- there are no untracked payloads, local metadata, nested directories, or
  symbolic links; and
- the version is not a Maven `-SNAPSHOT`.

Empty repository IDs written by local installation, `maven-metadata-local.xml`,
untracked classifier files, snapshots, malformed markers, unreadable paths, and
entry-limit truncation are fail-closed holds. Candidate fingerprints bind the
relative path, file names, sizes, modification times, and repository
attributions observed during the audit.

`scan_truncated` means the repository walk hit its entry bound and therefore
emits no reclaim candidates. `candidate_output_truncated` means the complete
aggregate was calculated but only the largest bounded candidate rows are
included. `truncated` is the union of those conditions.

`candidate_set_fingerprint` binds the canonical repository root plus every
candidate relative path, logical byte observation, and per-candidate metadata
fingerprint. It remains stable when only the output row limit changes. It is
evidence for a later exact approval gate, not authorization by itself.

The command does not delete files, invoke Maven, access remote repositories, or
claim physically reclaimable bytes. `remote_recoverable_bytes` is a logical
size observation. Before any later deletion, DiskSage must revalidate the
candidate fingerprint, verify that no process has the directory open, and bind
the exact action to separate user approval.

`--output` uses create-new semantics and, on Unix, mode `0600`; it refuses to
overwrite an existing report. Without `--output`, the complete JSON report is
printed to standard output.
