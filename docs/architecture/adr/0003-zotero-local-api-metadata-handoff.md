# ADR-0003: Zotero is a local metadata and original-source handoff

**Status:** Accepted
**Date:** 2026-08-20

## Context

The cloud-offload workflow needs a durable bibliographic record for standards and academic
sources, but a personal Zotero installation must not receive OAuth credentials or arbitrary local
paths. The Zotero application already exposes a loopback Local API. Its write contract is versioned
and may require a user-granted local API key. Zotero 10+ also exposes a local three-phase file
upload flow for stored attachments.

## Decision

DiskSage provides `disksage-zotero-local`. It accepts a bounded JSON manifest, validates titles,
creators, DOI/URL fields, and original source URLs in Rust, and defaults to a read-only preview.
Only an explicit `--execute` reads `ZOTERO_LOCAL_API_KEY` from the environment and POSTs metadata to
`http://127.0.0.1:23119/api/users/0/items`. The key is never accepted in the manifest or command
line. The manifest stores the original source URL and APA 7 rationale. An optional absolute
`fullTextPath` is accepted only for explicit execution; DiskSage rejects symlinks, regular-file
violations, and files over 4 GiB, computes MD5 and size before upload, and uses the local API's
three-phase stored-file flow. Preview mode never reads attachment contents. Source eviction remains
blocked until an independent cloud receipt exists.

## Consequences

- Zotero updates remain local and do not use OAuth or an external LLM.
- A missing local API key, unsupported Zotero version, or non-loopback endpoint fails closed before
  any write or attachment upload.
- The handoff is reproducible and auditable, while the cloud-transfer receipt remains the authority
  for cloud copies and source eviction.

## References

- [Zotero Local API](../../development/zotero-local-api.md)
- Zotero Documentation. (2026, July 29). *Zotero Local API*.
  https://www.zotero.org/support/dev/web_api/v3/local_api
- Zotero Documentation. (2026, July 29). *Zotero Web API file uploads*.
  https://www.zotero.org/support/dev/web_api/v3/file_upload
- DCMI Usage Board. (2020). *DCMI Metadata Terms*. DCMI Recommendation.
  https://www.dublincore.org/specifications/dublin-core/dcmi-terms/2020-01-20/
- Alam, M. M., & Wang, W. (2021). A comprehensive survey on the state-of-the-art data provenance
  approaches for security enforcement. *Journal of Computer Security, 29*(4), 423–446.
  https://doi.org/10.3233/JCS-200108

## Amendment: current Zotero capability and research handoff (2026-08-21)

The live loopback endpoint reports Zotero `9.0.6`, API version 3, and 8,312 items. A valid
metadata POST and a bounded invalid POST both return `400 Endpoint does not support method`;
the connector save route is not treated as an alternate write authority. No item, attachment,
or Zotero database content was changed. The research manifest therefore remains the explicit
APA 7 handoff until Zotero 10+ local-write authorization is available.

The manifest now includes NIST SP 800-209 for storage security and Buneman, Khanna, and Tan's
provenance characterization alongside PROV-O, DCAT, DCMI, and the existing provenance survey.
