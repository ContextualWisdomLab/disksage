# ADR-0003: Zotero is a local metadata and original-source handoff

**Status:** Accepted
**Date:** 2026-08-20

## Context

The cloud-offload workflow needs a durable bibliographic record for standards and academic
sources, but a personal Zotero installation must not receive OAuth credentials or arbitrary local
paths. The Zotero application already exposes a loopback Local API. Its write contract is versioned
and may require a user-granted local API key.

## Decision

DiskSage provides `disksage-zotero-local`. It accepts a bounded JSON manifest, validates titles,
creators, DOI/URL fields, and original source URLs in Rust, and defaults to a read-only preview.
Only an explicit `--execute` reads `ZOTERO_LOCAL_API_KEY` from the environment and POSTs metadata to
`http://127.0.0.1:23119/api/users/0/items`. The key is never accepted in the manifest or command
line. The manifest stores the original source URL and APA 7 rationale; it does not claim a full
text attachment was uploaded. A future attachment operation must have its own bounded, content
verified receipt.

## Consequences

- Zotero updates remain local and do not use OAuth or an external LLM.
- A missing local API key, unsupported Zotero version, or non-loopback endpoint fails closed before
  any write.
- The handoff is reproducible and auditable, while the cloud-transfer receipt remains the authority
  for cloud copies and source eviction.

## References

- [Zotero Local API](../../development/zotero-local-api.md)
- Zotero Documentation. (2026, July 29). *Zotero Local API*.
  https://www.zotero.org/support/dev/web_api/v3/local_api
- Zotero Documentation. (2026, July 29). *Zotero Web API file uploads*.
  https://www.zotero.org/support/dev/web_api/v3/file_upload
